use std::collections::{HashMap, HashSet};

use petgraph::acyclic::Acyclic;
use spacetimedb::{
    AnonymousViewContext, DbContext, Local, Query, ReducerContext, SpacetimeType, Table,
    ViewContext, reducer, view,
};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        connection::{
            action::try_exec_action,
            data::{ConnectionData, tab_connection_data, tab_connection_data__view},
        },
        node::{NodeHandle, NodeRead, NodeType},
    },
    raw_server::player::PermittedPlayer,
    registration::player::RegistrationRead,
    tm_match::leaderboard::MatchLeadearboardRead,
    user::UserRead,
};

pub(super) mod action;
pub(super) mod data;

#[spacetimedb::table(accessor= tab_connection,
    index(accessor=connection_exists,hash(columns=[origin_variant,target_variant,origin_id,target_id])),
    index(accessor=targets_of,hash(columns=[origin_variant,origin_id])),
    index(accessor=origins_of,hash(columns=[target_variant,target_id]))
)]
#[derive(Debug, Clone, Copy)]
pub struct TabConnection {
    // We need this that the Data variant can reference this.
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    origin_id: u32,
    target_id: u32,
    origin_variant: u8,
    target_variant: u8,

    connection_type: ConnectionType,
    status: ConnectionStatus,
}

impl TabConnection {
    pub(crate) fn connection_origin(&self) -> NodeHandle {
        NodeHandle::combine(self.origin_variant, self.origin_id)
    }

    pub(crate) fn connection_target(&self) -> NodeHandle {
        NodeHandle::combine(self.target_variant, self.target_id)
    }

    pub(crate) fn is_data(&self) -> bool {
        self.connection_type == ConnectionType::Data
    }

    pub(crate) fn is_wait(&self) -> bool {
        self.connection_type == ConnectionType::Wait
    }

    pub(crate) fn is_action(&self) -> bool {
        self.connection_type == ConnectionType::Action
    }

    pub(crate) fn resolve(&mut self) {
        self.status = ConnectionStatus::Resolved
    }

    pub(crate) fn is_resolved(&self) -> bool {
        self.status == ConnectionStatus::Resolved
    }

    pub(crate) fn instantiate(mut self, parent_id: u32) -> Self {
        self.parent_id = parent_id;
        self.id = 0;
        self
    }

    pub(crate) fn update_origin(&mut self, new_origin: u32) {
        self.origin_id = new_origin;
    }

    pub(crate) fn update_target(&mut self, new_target: u32) {
        self.target_id = new_target;
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Configuring,
    Configured,
    Resolved,
}

#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    Wait,
    Data,
    Action,
}

/// Since we need to check either way if the two thing have the same parent we can omit specifing the competition manually.
#[reducer]
pub fn connection_create(
    ctx: &ReducerContext,
    origin: NodeHandle,
    target: NodeHandle,
    setting: ConnectionType,
) -> Result<(), String> {
    if origin == target {
        return Err("Cannot connect a Node to itself.".into());
    }

    let from_comp = ctx.node_get_parent(origin)?;
    let to_comp = ctx.node_get_parent(target)?;

    if from_comp != to_comp {
        return Err(
            "Cannot add a connection where nodes are part of different competitions!".into(),
        );
    }

    if origin.is_template(ctx) != target.is_template(ctx) {
        return Err(
            "Not allowed to form a connection between template and non template nodes.".into(),
        );
    }

    ctx.auth_builder(from_comp)
        .permission(CompetitionPermissionsV1::COMPETITION_CONNECTION_EDIT)
        .authorize()?;

    let mut set = HashSet::new();
    set.insert(origin);
    set.insert(target);

    let (split_connection_from_variant, split_connection_from) = origin.split();
    let (split_connection_to_variant, split_connection_to) = target.split();
    if ctx
        .db
        .tab_connection()
        .connection_exists()
        .filter((
            split_connection_from_variant,
            split_connection_to_variant,
            split_connection_from,
            split_connection_to,
        ))
        .next()
        .is_some()
    {
        return Err("Parallel edges not allowed.".into());
    };

    let competition_connections = ctx
        .db
        .tab_connection()
        .parent_id()
        .filter(from_comp)
        .collect::<Vec<_>>();

    for connection in &competition_connections {
        set.insert(NodeHandle::combine(
            connection.origin_variant,
            connection.origin_id,
        ));
        set.insert(NodeHandle::combine(
            connection.target_variant,
            connection.target_id,
        ));
    }

    log::error!("{set:?}");

    let mut map = HashMap::with_capacity(set.len());
    let mut graph = petgraph::graph::Graph::new();
    for set_entry in set.into_iter() {
        let index = graph.add_node(set_entry);
        map.insert(set_entry, index);
    }
    log::error!("{map:?}");

    let edge_extension = competition_connections
        .into_iter()
        .map(|c| {
            (
                *map.get(&c.connection_origin()).unwrap(),
                *map.get(&c.connection_target()).unwrap(),
                c.connection_type,
            )
        })
        .collect::<Vec<_>>();

    graph.extend_with_edges(edge_extension);

    log::error!("{graph:?}");
    let mut graph = Acyclic::try_from_graph(graph).map_err(|e| format!("{e:?}"))?;
    graph
        .try_add_edge(
            *map.get(&origin).unwrap(),
            *map.get(&target).unwrap(),
            setting,
        )
        .map_err(|e| format!("{e:?}"))?;

    let (origin_variant, origin_id) = origin.split();
    let (target_variant, target_id) = target.split();
    let connection = ctx.db.tab_connection().try_insert(TabConnection {
        id: 0,
        parent_id: from_comp,
        origin_id,
        target_id,
        origin_variant,
        target_variant,
        connection_type: setting,
        status: ConnectionStatus::Configuring,
    })?;

    //If we insert Data Settings we also need to add a row in the data table.
    match connection.connection_type {
        ConnectionType::Wait => (),
        ConnectionType::Data => {
            ctx.db
                .tab_connection_data()
                .try_insert(ConnectionData::new(connection.id, connection.parent_id))?;
        }
        ConnectionType::Action => {
            todo!()
        }
    }

    Ok(())
}

#[derive(Debug, SpacetimeType)]
pub struct CompetitionConnection {
    id: u32,

    origin: NodeHandle,
    target: NodeHandle,

    connection_type: ConnectionType,
    status: ConnectionStatus,
}

impl CompetitionConnection {
    pub(crate) fn is_action(&self) -> bool {
        self.connection_type == ConnectionType::Action
    }
}

impl From<TabConnection> for CompetitionConnection {
    fn from(v: TabConnection) -> Self {
        CompetitionConnection {
            origin: NodeHandle::combine(v.origin_variant, v.origin_id),
            target: NodeHandle::combine(v.target_variant, v.target_id),
            connection_type: v.connection_type,
            id: v.id,
            status: v.status,
        }
    }
}

/* #[view(accessor=competition_connection,public)]
pub fn competition_connection(
    ctx: &AnonymousViewContext, /* competition_id: u32 */
) -> Vec<CompetitionConnection> {
    let competition_id = 1u32;

    ctx.db
        .tab_connection()
        .parent_id()
        .filter(competition_id)
        .map(CompetitionConnection::from)
        .collect()
} */

#[view(accessor=my_connections,public)]
fn my_connections(
    ctx: &ViewContext, /* competition_id: u32 */
) -> impl Query<CompetitionConnection> {
    /* let Ok(user) = ctx.user_id() else {
        log::warn!(
            "Non user account has tried to call protected view: {}",
            ctx.sender()
        );
        return Vec::new();
    }; */

    let competition_id = 1u32;

    //TODO access control for only permitted users. e.g. walk competition tree for permission.

    ctx.from.tab_connection()
}

pub(crate) fn internal_graph_resolution_node_finished(
    ctx: &ReducerContext,
    trigger: NodeHandle,
) -> Result<(), String> {
    // Get the outgoing connections from the node that just finished (trigger).
    let affected_connections = ctx
        .db
        .tab_connection()
        .targets_of()
        .filter(trigger.split())
        .map(|mut c| {
            c.resolve();
            ctx.db.tab_connection().id().update(c);
            CompetitionConnection::from(c)
        });

    for affected_connection in affected_connections {
        // If that connection is a action connection it cannot be the last missing connection
        // because it is not counted in the first place so we can safely skip it.
        if affected_connection.is_action() {
            try_exec_action(affected_connection.id, affected_connection.target, ctx);

            // Action connections dont influence anything else.
            continue;
        }

        let pending_connections = ctx
            .db
            .tab_connection()
            .origins_of()
            .filter(affected_connection.target.split())
            // Action connections dont influence the implicit advance flow.
            // If the connection is resolved we discard it so if everything is resolved we have an empty array.
            .filter(|c| !c.is_action() && !c.is_resolved())
            .collect::<Vec<_>>();

        // When no more pending connections are left it is safe to implicitly start depending nodes.
        if pending_connections.is_empty() {
            log::warn!("The node can be started now.");
            if let Err(error) = affected_connection.target.ready(ctx) {
                //TODO maybe add a table for node problems?
                // maybe there also should be a intended to progress state in the nodes.
                log::error!(
                    "Implicit Flow: Node should have been ready but action failed. Error: {error}"
                )
            };
        } else {
            log::info!(
                "There are still nodes that are not finished!, Pending Nodes: {pending_connections:?}"
            );
        }
    }

    Ok(())
}

pub(crate) trait ConnectionRead {
    fn connection_filter_permitted_players(
        &self,
        connection: TabConnection,
    ) -> Vec<PermittedPlayer>;
}
impl<Db: DbContext> ConnectionRead for Db {
    fn connection_filter_permitted_players(
        &self,
        connection: TabConnection,
    ) -> Vec<PermittedPlayer> {
        match connection.connection_origin() {
            NodeHandle::MatchV1(m) => {
                let rules = self
                    .db_read_only()
                    .tab_connection_data()
                    .connection_id()
                    .find(connection.id)
                    .unwrap();

                let leaderboard = self.match_leaderboard(m, 0);

                //TODO maybe factor this out into a trait and impl it for the respective thing
                // maybe we also need to split the data portion out into separate tables for each connection.
                rules.apply_match(leaderboard)
            }
            NodeHandle::CompetitionV1(c) => todo!(),
            //NodeHandle::MonitoringV1(_) => todo!(),
            NodeHandle::ServerV1(_) => todo!(),
            NodeHandle::ScheduleV1(_) => todo!(),
            NodeHandle::PortalV1(_) => todo!(),
            NodeHandle::RegistrationV1(r) => {
                let rules = self
                    .db_read_only()
                    .tab_connection_data()
                    .connection_id()
                    .find(connection.id)
                    .unwrap();

                let leaderboard = self.registration_player(r);

                //TODO maybe factor this out into a trait and impl it for the respective thing
                // maybe we also need to split the data portion out into separate tables for each connection.
                rules
                    .apply_registration(leaderboard)
                    .into_iter()
                    .map(|p| {
                        PermittedPlayer::new(self.user_account_from_id(p.user_id), false, false)
                    })
                    .collect()
            }
        }
    }
}
/* pub(crate) trait ConnectionWrite: ConnectionRead {}
impl<Db: DbContext<DbView = Local>> ConnectionWrite for Db {} */
