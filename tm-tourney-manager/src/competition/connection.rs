use std::collections::{HashMap, HashSet};

use petgraph::acyclic::Acyclic;
use spacetimedb::{DbContext, ReducerContext, SpacetimeType, Table, ViewContext, reducer, view};

use crate::{
    authorization::Authorization,
    competition::{
        connection::connection_data::{
            CompetitionConnectionData, tab_competition_connection_data,
            tab_competition_connection_data__view,
        },
        tab_competition, tab_competition__view,
    },
    portal::tab_portal,
    project::permissions::ProjectPermissionsV1,
    raw_server::player::PermittedPlayer,
    registration::tab_registration,
    scheduling::tab_schedule,
    tm_match::{
        leaderboard::{match_leaderboard, tab_tm_match_round_player__view},
        match_set_preparation, tab_tm_match, tab_tm_match__view,
    },
};

pub(super) mod connection_data;
pub(crate) mod node_position;

#[spacetimedb::table(accessor= tab_competition_connection,
    index(accessor=connection_exists,hash(columns=[connection_from_variant,connection_to_variant,connection_from,connection_to])),
    index(accessor=target_nodes_of,hash(columns=[connection_from_variant,connection_from])),
    index(accessor=origin_nodes_of,hash(columns=[connection_to_variant,connection_to]))
)]
/* #[spacetimedb::table(accessor= tab_competition_connection_template,
    index(accessor=connection_exists,hash(columns=[connection_from_variant,connection_to_variant,connection_from,connection_to])),
    index(accessor=target_nodes_of,hash(columns=[connection_from_variant,connection_from])),
    index(accessor=origin_nodes_of,hash(columns=[connection_to_variant,connection_to]))
)] */
#[derive(Debug, Clone, Copy)]
pub struct TabCompetitionConnection {
    // We need this that the Data variant can reference this.
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    //Maybe not necessary if we can expose another view with arg or something like that.
    project_id: u32,

    connection_from: u32,
    connection_to: u32,
    connection_from_variant: u8,
    connection_to_variant: u8,

    connection_settings: ConnectionSettings,
    /* //Wheter the connection has served its purpose and can be skipped.
    //template: bool, */
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

    pub(crate) fn is_waiting(&self) -> bool {
        self.connection_settings == ConnectionSettings::Waiting
    }

    pub(crate) fn get_permitted_players(self, ctx: &ViewContext) -> Vec<PermittedPlayer> {
        if self.is_waiting() {
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
            NodeKindHandle::SchedulingV1(_) => todo!(),
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
    Waiting,
    Data,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, SpacetimeType, Hash)]
#[non_exhaustive]
pub enum NodeKindHandle {
    MatchV1(u32),
    CompetitionV1(u32),
    MonitoringV1(u32),
    ServerV1(u32),
    SchedulingV1(u32),
    PortalV1(u32),
    RegistrationV1(u32),
}

// This is done because of a petgraph trait bound.
impl Default for NodeKindHandle {
    fn default() -> Self {
        log::error!(
            "Tried to call the deafault implementation of NodeKindHandle.
            This should not be possible and is only implemented because of a petgraph trait bound."
        );
        panic!()
    }
}

impl NodeKindHandle {
    pub(crate) fn get_competition(&self, ctx: &ReducerContext) -> Result<u32, String> {
        match self {
            NodeKindHandle::MatchV1(m) => {
                if let Some(ma) = ctx.db.tab_tm_match().id().find(m) {
                    Ok(ma.get_comp_id())
                } else {
                    Err("Match couldnt be found.".into())
                }
            }
            NodeKindHandle::CompetitionV1(c) => {
                if let Some(co) = ctx.db.tab_competition().id().find(c) {
                    let id = co.get_comp_id();
                    if id != 0 {
                        Ok(id)
                    } else {
                        Err("Compeittion without Parent cannot be part of a connection".into())
                    }
                } else {
                    Err("Competition could not be found".into())
                }
            }
            NodeKindHandle::SchedulingV1(sched) => {
                if let Some(ma) = ctx.db.tab_schedule().scheduled_id().find(*sched as u64) {
                    Ok(ma.parent_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(portal_id) => {
                if let Some(portal) = ctx.db.tab_portal().id().find(*portal_id) {
                    Ok(portal.get_comp_id())
                } else {
                    Err("Portal could not be found.".into())
                }
            }
            NodeKindHandle::RegistrationV1(reg) => {
                if let Some(reg) = ctx.db.tab_registration().id().find(reg) {
                    Ok(reg.get_comp_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
        }
    }

    /// Safety: can only be called when you know the competiiton exists
    pub(crate) fn get_project(&self, ctx: &ReducerContext) -> u32 {
        match self {
            NodeKindHandle::MatchV1(m) => {
                if let Some(ma) = ctx.db.tab_tm_match().id().find(m) {
                    ma.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::CompetitionV1(c) => {
                if let Some(co) = ctx.db.tab_competition().id().find(c) {
                    co.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::SchedulingV1(sched) => {
                if let Some(ma) = ctx.db.tab_schedule().scheduled_id().find(*sched as u64) {
                    ma.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(port) => {
                if let Some(portal) = ctx.db.tab_portal().id().find(port) {
                    portal.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::RegistrationV1(reg) => {
                if let Some(reg) = ctx.db.tab_registration().id().find(reg) {
                    reg.get_project()
                } else {
                    u32::MAX
                }
            }
        }
    }

    pub(crate) fn split(self) -> (u8, u32) {
        match self {
            NodeKindHandle::MatchV1(m) => (1, m),
            NodeKindHandle::CompetitionV1(c) => (2, c),
            NodeKindHandle::SchedulingV1(s) => (3, s),
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(p) => (6, p),
            NodeKindHandle::RegistrationV1(r) => (7, r),
        }
    }

    pub(crate) fn combine(variant: u8, value: u32) -> Self {
        match variant {
            1 => Self::MatchV1(value),
            2 => Self::CompetitionV1(value),
            3 => Self::SchedulingV1(value),
            6 => Self::PortalV1(value),
            7 => Self::RegistrationV1(value),
            _ => unreachable!(),
        }
    }

    pub(crate) fn is_template(&self, ctx: &ReducerContext) -> bool {
        match self {
            NodeKindHandle::MatchV1(m) => {
                let node = ctx.db.tab_tm_match().id().find(m).unwrap();
                node.is_template()
            }
            NodeKindHandle::CompetitionV1(c) => {
                let node = ctx.db.tab_competition().id().find(c).unwrap();
                node.is_template()
            }
            NodeKindHandle::SchedulingV1(sched) => {
                let node = ctx
                    .db
                    .tab_schedule()
                    .scheduled_id()
                    .find(*sched as u64)
                    .unwrap();
                node.is_template()
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(portal_id) => {
                let node = ctx.db.tab_portal().id().find(portal_id).unwrap();
                node.is_template()
            }
            NodeKindHandle::RegistrationV1(reg) => {
                let node = ctx.db.tab_registration().id().find(reg).unwrap();
                node.is_template()
            }
        }
    }
}

pub trait NodeType {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String>;
}

impl NodeType for NodeKindHandle {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String> {
        match self {
            NodeKindHandle::MatchV1(match_id) => match_set_preparation(ctx, *match_id),
            NodeKindHandle::CompetitionV1(_) => todo!(),
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::SchedulingV1(_) => todo!(),
            NodeKindHandle::PortalV1(_) => todo!(),
            NodeKindHandle::RegistrationV1(_) => todo!(),
        }
    }
}

/// Since we need to check either way if the two thing have the same parent we can omit specifing the competition manually.
#[reducer]
pub fn connection_create(
    ctx: &ReducerContext,
    connection_from: NodeKindHandle,
    connection_to: NodeKindHandle,
    setting: ConnectionSettings,
) -> Result<(), String> {
    let account_id = ctx.get_user()?.account_id;

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

    let project_id = connection_from.get_project(ctx);

    ctx.auth_builder(project_id, account_id)?
        .permission(ProjectPermissionsV1::COMPETITION_CONNECTION_EDIT)
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
            project_id,
            parent_id: from_comp,
            connection_from,
            connection_to,
            connection_from_variant,
            connection_to_variant,
            connection_settings: setting,
        })?;

    //If we insert Data Settings we also need to add a row in the data table.
    match connection.connection_settings {
        ConnectionSettings::Waiting => (),
        ConnectionSettings::Data => {
            ctx.db
                .tab_competition_connection_data()
                .try_insert(CompetitionConnectionData::new(
                    connection.id,
                    connection.parent_id,
                ))?;
        }
    }

    Ok(())
}

#[derive(Debug, SpacetimeType)]
pub struct CompetitionConnection {
    project_id: u32,
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
            project_id: v.project_id,
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
            project_id: t.project_id,
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
