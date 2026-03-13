use std::collections::{HashMap, HashSet};

use petgraph::acyclic::Acyclic;
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, view};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        connection::data::{
            CompetitionConnectionData, tab_competition_connection_data,
            tab_competition_connection_data__view,
        },
        node::{NodeKindHandle, NodeType},
    },
    raw_server::player::PermittedPlayer,
    tm_match::leaderboard::match_leaderboard,
};

pub(super) mod action;
pub(super) mod data;

#[spacetimedb::table(accessor= tab_competition_connection,
    index(accessor=connection_exists,hash(columns=[connection_from_variant,connection_to_variant,connection_from,connection_to])),
    index(accessor=target_nodes_of,hash(columns=[connection_from_variant,connection_from])),
    index(accessor=origin_nodes_of,hash(columns=[connection_to_variant,connection_to]))
)]
#[derive(Debug, Clone, Copy)]
pub struct TabCompetitionConnection {
    // We need this that the Data variant can reference this.
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    connection_from: u32,
    connection_to: u32,
    connection_from_variant: u8,
    connection_to_variant: u8,

    connection_settings: ConnectionSettings,
    connection_settings_ready: bool,
}

impl TabCompetitionConnection {
    pub(crate) fn connection_origin(&self) -> NodeKindHandle {
        NodeKindHandle::combine(self.connection_from_variant, self.connection_from)
    }

    pub(crate) fn connection_target(&self) -> NodeKindHandle {
        NodeKindHandle::combine(self.connection_to_variant, self.connection_to)
    }

    pub(crate) fn is_data(&self) -> bool {
        self.connection_settings == ConnectionSettings::Data
    }

    pub(crate) fn is_wait(&self) -> bool {
        self.connection_settings == ConnectionSettings::Wait
    }

    pub(crate) fn is_action(&self) -> bool {
        self.connection_settings == ConnectionSettings::Action
    }

    pub(crate) fn get_permitted_players(self, ctx: &ViewContext) -> Vec<PermittedPlayer> {
        if self.is_wait() {
            return Vec::new();
        }

        //TODO apply filter from the connection data settings.
        self.get_permitted_players_filter(ctx)
    }

    pub(crate) fn get_permitted_players_filter(&self, ctx: &ViewContext) -> Vec<PermittedPlayer> {
        match self.connection_origin() {
            NodeKindHandle::MatchV1(m) => {
                let rules = ctx
                    .db
                    .tab_competition_connection_data()
                    .connection_id()
                    .find(self.id)
                    .unwrap();

                let leaderboard = match_leaderboard(&ctx.as_anonymous_read_only());

                //TODO maybe factor this out into a trait and impl it for the respective thing
                // maybe we also need to split the data portion out into separate tables for each connection.
                rules.apply_match(leaderboard)
            }
            NodeKindHandle::CompetitionV1(c) => todo!(),
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::ScheduleV1(_) => todo!(),
            NodeKindHandle::PortalV1(_) => todo!(),
            NodeKindHandle::RegistrationV1(_) => todo!(),
        }
    }

    pub(crate) fn instantiate(mut self, parent_id: u32) -> Self {
        self.parent_id = parent_id;
        self.id = 0;
        self
    }

    pub(crate) fn update_origin(&mut self, new_origin: u32) {
        self.connection_from = new_origin;
    }

    pub(crate) fn update_target(&mut self, new_target: u32) {
        self.connection_to = new_target;
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionSettings {
    Wait,
    Data,
    Action,
}

/// Since we need to check either way if the two thing have the same parent we can omit specifing the competition manually.
#[reducer]
pub fn connection_create(
    ctx: &ReducerContext,
    connection_from: NodeKindHandle,
    connection_to: NodeKindHandle,
    setting: ConnectionSettings,
) -> Result<(), String> {
    if connection_from == connection_to {
        return Err("Cannot connect a Node to itself.".into());
    }

    let from_comp = connection_from.get_competition(ctx)?;
    let to_comp = connection_to.get_competition(ctx)?;

    if from_comp != to_comp {
        return Err(
            "Cannot add a connection where nodes are part of different competitions!".into(),
        );
    }

    if connection_from.is_template(ctx) != connection_to.is_template(ctx) {
        return Err(
            "Not allowed to form a connection between template and non template nodes.".into(),
        );
    }

    ctx.auth_builder(from_comp)
        .permission(CompetitionPermissionsV1::COMPETITION_CONNECTION_EDIT)
        .authorize()?;

    let mut set = HashSet::new();
    set.insert(connection_from);
    set.insert(connection_to);

    let (split_connection_from_variant, split_connection_from) = connection_from.split();
    let (split_connection_to_variant, split_connection_to) = connection_to.split();
    if ctx
        .db
        .tab_competition_connection()
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
        .tab_competition_connection()
        .parent_id()
        .filter(from_comp)
        .collect::<Vec<_>>();

    for connection in &competition_connections {
        set.insert(NodeKindHandle::combine(
            connection.connection_from_variant,
            connection.connection_from,
        ));
        set.insert(NodeKindHandle::combine(
            connection.connection_to_variant,
            connection.connection_to,
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
                c.connection_settings,
            )
        })
        .collect::<Vec<_>>();

    graph.extend_with_edges(edge_extension);

    log::error!("{graph:?}");
    let mut graph = Acyclic::try_from_graph(graph).map_err(|e| format!("{e:?}"))?;
    graph
        .try_add_edge(
            *map.get(&connection_from).unwrap(),
            *map.get(&connection_to).unwrap(),
            setting,
        )
        .map_err(|e| format!("{e:?}"))?;

    let (connection_from_variant, connection_from) = connection_from.split();
    let (connection_to_variant, connection_to) = connection_to.split();
    let connection = ctx
        .db
        .tab_competition_connection()
        .try_insert(TabCompetitionConnection {
            id: 0,
            parent_id: from_comp,
            connection_from,
            connection_to,
            connection_from_variant,
            connection_to_variant,
            connection_settings: setting,
            connection_settings_ready: false,
        })?;

    //If we insert Data Settings we also need to add a row in the data table.
    match connection.connection_settings {
        ConnectionSettings::Wait => (),
        ConnectionSettings::Data => {
            ctx.db
                .tab_competition_connection_data()
                .try_insert(CompetitionConnectionData::new(
                    connection.id,
                    connection.parent_id,
                ))?;
        }
        ConnectionSettings::Action => {
            todo!()
        }
    }

    Ok(())
}

#[derive(Debug, SpacetimeType)]
pub struct CompetitionConnection {
    competition_id: u32,

    connection_from: NodeKindHandle,
    connection_to: NodeKindHandle,

    connection_settings: ConnectionSettings,
}

#[view(accessor=competition_connection,public)]
pub fn competition_connection(
    ctx: &ViewContext, /* competition_id: u32 */
) -> Vec<CompetitionConnection> {
    let competition_id = 1u32;

    ctx.db
        .tab_competition_connection()
        .parent_id()
        .filter(competition_id)
        .map(|v| CompetitionConnection {
            competition_id: v.parent_id,
            connection_from: NodeKindHandle::combine(v.connection_from_variant, v.connection_from),
            connection_to: NodeKindHandle::combine(v.connection_to_variant, v.connection_to),
            connection_settings: v.connection_settings,
        })
        .collect()
}

pub fn internal_graph_resolution_node_finished(
    ctx: &ReducerContext,
    trigger: NodeKindHandle,
) -> Result<(), String> {
    let affected_connections = ctx
        .db
        .tab_competition_connection()
        .target_nodes_of()
        .filter(trigger.split())
        .map(|t| CompetitionConnection {
            competition_id: t.parent_id,
            connection_from: NodeKindHandle::combine(t.connection_from_variant, t.connection_from),
            connection_to: NodeKindHandle::combine(t.connection_to_variant, t.connection_to),
            connection_settings: t.connection_settings,
        });

    for affected_connection in affected_connections.map(|c| c.connection_to) {
        let pending_connections = ctx
            .db
            .tab_competition_connection()
            .origin_nodes_of()
            .filter(affected_connection.split())
            .collect::<Vec<_>>();
        if pending_connections.is_empty() {
            log::warn!("The node can be started now.");
            if let Err(error) = affected_connection.ready(ctx) {
                //TODO maybe add a table for node problems?
                // maybe there also should be a intended to progress state in the nodes.
                log::error!("Node shoud have been started but could because: {error}")
            };
        } else {
            log::info!(
                "There are still nodes that are not finished!, Pending Nodes: {pending_connections:?}"
            );
        }
    }

    Ok(())
}
