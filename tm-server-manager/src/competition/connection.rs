use std::collections::{HashMap, HashSet};

use petgraph::acyclic::Acyclic;
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, view};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        connection::{
            action::{TabConnectionAction, tab_connection_action, try_exec_action},
            data::{ConnectionData, tab_connection_data, tab_connection_data__view},
        },
        node::{NodeHandle, NodeRead, NodeWrite},
    },
    input::InputRead,
    leaderboard::LbEntry,
    output::OutputRead,
    schedule::ScheduleWrite,
    tm_match::MatchWrite,
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

    #[index(btree)]
    parent_id: u32,

    origin_id: u32,
    target_id: u32,
    origin_variant: u8,
    target_variant: u8,

    kind: ConnectionKind,
    status: ConnectionStatus,
}

impl TabConnection {
    pub(crate) fn origin(&self) -> NodeHandle {
        NodeHandle::combine(self.origin_variant, self.origin_id)
    }

    pub(crate) fn target(&self) -> NodeHandle {
        NodeHandle::combine(self.target_variant, self.target_id)
    }

    pub(crate) fn is_data(&self) -> bool {
        self.kind == ConnectionKind::Data
    }

    pub(crate) fn is_wait(&self) -> bool {
        self.kind == ConnectionKind::Wait
    }

    pub(crate) fn is_action(&self) -> bool {
        self.kind == ConnectionKind::Action
    }

    pub(crate) fn resolve(&mut self) {
        if self.status == ConnectionStatus::Configured {
            self.status = ConnectionStatus::Resolved
        } else {
            log::info!("Connection was not in configured state but was requested to be resolved.")
        }
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
pub enum ConnectionKind {
    Wait,
    Data,
    Action,
}

impl ConnectionKind {
    pub(crate) fn is_data(&self) -> bool {
        matches!(self, ConnectionKind::Data)
    }
    pub(crate) fn is_wait(&self) -> bool {
        matches!(self, ConnectionKind::Wait)
    }
    pub(crate) fn is_action(&self) -> bool {
        matches!(self, ConnectionKind::Action)
    }
}

/// Since we need to check either way if the two thing have the same parent we can omit specifing the competition manually.
#[reducer]
fn connection_create(
    ctx: &ReducerContext,
    origin: NodeHandle,
    target: NodeHandle,
    kind: ConnectionKind,
) -> Result<(), String> {
    if origin == target {
        return Err("Cannot connect a Node to itself.".into());
    }

    let origin_parent = ctx.node_get_parent(origin)?;
    let target_parent = ctx.node_get_parent(target)?;

    if origin_parent != target_parent {
        return Err(
            "Cannot add a connection where nodes are part of different competitions!".into(),
        );
    }

    let parent = origin_parent;

    ConnectionCombination {
        origin,
        target,
        kind,
    }
    .validate()?;

    if ctx.node_is_template(origin) != ctx.node_is_template(target) {
        return Err(
            "Not allowed to form a connection between template and non template nodes.".into(),
        );
    }

    ctx.auth_builder(origin_parent)
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
        .filter(origin_parent)
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

    let mut map = HashMap::with_capacity(set.len());
    let mut graph = petgraph::graph::Graph::new();
    for set_entry in set.into_iter() {
        let index = graph.add_node(set_entry);
        map.insert(set_entry, index);
    }

    let edge_extension = competition_connections
        .into_iter()
        .map(|c| {
            (
                *map.get(&c.origin()).unwrap(),
                *map.get(&c.target()).unwrap(),
                c.kind,
            )
        })
        .collect::<Vec<_>>();

    graph.extend_with_edges(edge_extension);

    let mut graph = Acyclic::try_from_graph(graph).map_err(|e| format!("{e:?}"))?;
    graph
        .try_add_edge(*map.get(&origin).unwrap(), *map.get(&target).unwrap(), kind)
        .map_err(|e| format!("{e:?}"))?;

    let (origin_variant, origin_id) = origin.split();
    let (target_variant, target_id) = target.split();
    let connection = ctx.db.tab_connection().try_insert(TabConnection {
        id: 0,
        parent_id: origin_parent,
        origin_id,
        target_id,
        origin_variant,
        target_variant,
        kind,
        status: ConnectionStatus::Configuring,
    })?;

    //If we insert Data Settings we also need to add a row in the data table.
    match connection.kind {
        ConnectionKind::Wait => (),
        ConnectionKind::Data => {
            ctx.db
                .tab_connection_data()
                .try_insert(ConnectionData::new(connection.id, connection.parent_id))?;
        }
        ConnectionKind::Action => {
            ctx.db
                .tab_connection_action()
                .try_insert(TabConnectionAction::new(target, parent, connection.id)?)?;
        }
    }

    Ok(())
}

#[reducer]
fn connection_configured(ctx: &ReducerContext, connection_id: u32) -> Result<(), String> {
    let Some(mut connection) = ctx.db.tab_connection().id().find(connection_id) else {
        return Err("Connection not found.".into());
    };

    ctx.auth_builder(connection.parent_id)
        .permission(CompetitionPermissionsV1::COMPETITION_CONNECTION_EDIT)
        .authorize()?;

    if connection.status != ConnectionStatus::Configuring {
        return Err("Wrong status to configure".into());
    }

    connection.status = ConnectionStatus::Configured;

    ctx.db.tab_connection().id().update(connection);

    Ok(())
}

#[derive(Debug, SpacetimeType)]
pub struct CompetitionConnection {
    id: u32,
    competition_id: u32,

    origin: NodeHandle,
    target: NodeHandle,

    kind: ConnectionKind,
    status: ConnectionStatus,
}

impl CompetitionConnection {
    pub(crate) fn is_action(&self) -> bool {
        self.kind == ConnectionKind::Action
    }
}

impl From<TabConnection> for CompetitionConnection {
    fn from(v: TabConnection) -> Self {
        CompetitionConnection {
            origin: NodeHandle::combine(v.origin_variant, v.origin_id),
            target: NodeHandle::combine(v.target_variant, v.target_id),
            kind: v.kind,
            id: v.id,
            status: v.status,
            competition_id: v.parent_id,
        }
    }
}

#[view(accessor=unstable_competition_connection,public)]
pub fn unstable_competition_connection(
    ctx: &ViewContext, /* competition_id: u32 */
) -> Vec<CompetitionConnection> {
    let competition_id = 1u32;

    ctx.db
        .tab_connection()
        .parent_id()
        .filter(0..u32::MAX)
        .map(CompetitionConnection::from)
        .collect()
}

/* #[view(accessor=my_connections,public)]
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
} */

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

            let result = match affected_connection.target {
                NodeHandle::MatchV1(match_id) => ctx.match_set_preparation(match_id, ctx.timestamp),
                NodeHandle::CompetitionV1(c) => {
                    let inputs = ctx.inputs_in_parent(c);

                    for input in inputs {
                        let result = internal_graph_resolution_node_finished(
                            ctx,
                            NodeHandle::InputV1(input.id),
                        );

                        if let Err(error) = result {
                            log::error!(
                                "Implicit Flow: Node should have been ready but action failed. Error: {error}"
                            );
                        }
                    }

                    Ok(())
                }
                NodeHandle::ScheduleV1(s) => ctx.schedule_start_relative(s, ctx.timestamp),
                NodeHandle::ServerV1(_) => unreachable!(),
                NodeHandle::RegistrationV1(_) => unreachable!(),
                NodeHandle::InputV1(n) => {
                    internal_graph_resolution_node_finished(ctx, NodeHandle::InputV1(n))
                }
                NodeHandle::OutputV1(n) => {
                    let output = ctx.output_find(n);
                    if output.is_err() {
                        log::error!("Hit a output node but it does not exist!");
                        return Ok(());
                    };
                    let output = output.unwrap();

                    internal_graph_resolution_node_finished(
                        ctx,
                        NodeHandle::CompetitionV1(output.get_comp_id()),
                    )
                }
                NodeHandle::LeaderboardV1(n) => {
                    // We can pass this through since the leadearboard has no state by itself.
                    // This means that if matches dpeend on it they will be triggered and if its
                    // a lead leadearboard where nothing is connected to it will just trigger nothing.
                    internal_graph_resolution_node_finished(ctx, NodeHandle::LeaderboardV1(n))
                }
            };

            if let Err(error) = result {
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
    /* fn connection_filter_permitted_players(
        &self,
        connection: TabConnection,
    ) -> Vec<PermittedPlayer>; */

    //fn connection_receive_leaderboard(&self, connection: TabConnection) -> Vec<PermittedPlayer>;

    fn connection_resolve_leaderboard(
        &self,
        connection: TabConnection,
        data: Vec<LbEntry>,
    ) -> Vec<LbEntry>;
}
impl<Db: spacetimedb::CtxDbRead> ConnectionRead for Db {
    fn connection_resolve_leaderboard(
        &self,
        connection: TabConnection,
        data: Vec<LbEntry>,
    ) -> Vec<LbEntry> {
        // Prevent the method from misuse because it is impossible to form a lb from a non data connection.
        if !connection.is_data() {
            log::error!("Wanted to filter a connection but it was not a data connection.");
            return Vec::new();
        }

        let rules = self
            .db_read_only()
            .tab_connection_data()
            .connection_id()
            .find(connection.id)
            .unwrap();

        //let players = self.node_resolve_input_data(connection.origin());

        let players = rules.apply_filter(data, self);
        players
    }
}
/* pub(crate) trait ConnectionWrite: ConnectionRead {}
impl<Db: spacetimedb::CtxDbWrite<DbView = Local>> ConnectionWrite for Db {} */

// Connection Rules
// Schedule:
// -> as origin: only wait and action connection.
// -> as target: only wait (also has to be set to relative).
// Competition:
// -> as origin: data and wait.
// -> as target: data and wait.
// Input:
// -> as origin: everything.
// -> as target: not allowed.
// Output:
// -> as origin: not allowed..
// -> as target: data and wait.
// Match:
// -> as origin: Everything.
// -> as target: Everything.
// Registration:
// -> as origin: Everything.
// -> as target: Action.
// Server:
// -> as origin: not allowed.
// -> as target: not allowed.
// Leadarboard: TODO

struct ConnectionCombination {
    origin: NodeHandle,
    target: NodeHandle,
    kind: ConnectionKind,
}

impl ConnectionCombination {
    fn validate(self) -> Result<(), String> {
        let origin = self.origin;
        let target = self.target;
        let kind = self.kind;

        if target.is_input() {
            return Err("Input cannot be target.".into());
        }
        if origin.is_output() {
            return Err("Output cannot be origin.".into());
        }
        if origin.is_server() || target.is_server() {
            return Err("Server cannot be involved in connection.".into());
        }
        if origin.is_schedule() && kind.is_data() {
            return Err("Cannot have a schedule with data connection".into());
        }

        if kind.is_action() {
            if target.is_match() || target.is_registration() {
                return Ok(());
            }

            return Err("Cannot put a action connection here.".into());
        }

        if origin.is_match() || target.is_match() {
            return Ok(());
        }

        if origin.is_schedule() && target.is_match() {
            return Ok(());
        }

        //TODO cover all cases and make it reject by default.
        return Ok(());

        /* Err(format!(
            "Unhandled Case: Combination of origin: {:?} and target: {:?} is not allowed.",
            origin, target
        )) */
    }
}

#[reducer]
fn connection_delete(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(connection) = ctx.db.tab_connection().id().find(id) else {
        return Err(String::new());
    };
    ctx.auth_builder(connection.parent_id)
        .permission(CompetitionPermissionsV1::COMPETITION_CONNECTION_EDIT)
        .authorize()?;

    ctx.connection_delete(id)
}
