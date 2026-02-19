use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, view};

use crate::{
    authorization::Authorization,
    competition::{
        connection::connection_data::{CompetitionConnectionData, tab_competition_connection_data},
        tab_competition,
    },
    tm_match::tab_tm_match,
    scheduling::tab_schedule,
};

pub(super) mod connection_data;
pub(crate) mod node_position;

#[spacetimedb::table(name = tab_competition_connection,
    index(name=connection_exists,hash(columns=[connection_from_variant,connection_to_variant,connection_from,connection_to])),
)]
#[derive(Debug, Clone, Copy)]
pub struct TabCompetitionConnection {
    // We need this that the Data variant can reference this.
    #[auto_inc]
    #[primary_key]
    id: u32,

    #[index(btree)]
    competition_id: u32,

    //Maybe not necessary if we can expose another view with arg or something like that.
    tournament_id: u32,

    connection_from: u32,
    connection_to: u32,
    connection_from_variant: u8,
    connection_to_variant: u8,

    connection_settings: ConnectionSettings,

    //Wheter the connection has served its purpose and can be skipped.
    resolved: bool,
}

impl TabCompetitionConnection {
    pub(crate) fn node_from(&self) -> NodeKindHandle {
        NodeKindHandle::combine(self.connection_from_variant, self.connection_from)
    }

    pub(crate) fn node_to(&self) -> NodeKindHandle {
        NodeKindHandle::combine(self.connection_to_variant, self.connection_to)
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub enum ConnectionSettings {
    Waiting,
    Data,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum NodeKindHandle {
    MatchV1(u32),
    CompetitionV1(u32),
    MonitoringV1(u32),
    ServerV1(u32),
    SchedulingV1(u32),
}

impl NodeKindHandle {
    fn get_competition(&self, ctx: &ReducerContext) -> Result<u32, String> {
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
                    if let Some(id) = co.get_comp_id() {
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
                    Ok(ma.get_comp_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
        }
    }

    /// Safety: can only be called when you know the competiiton exists
    fn get_tournament(&self, ctx: &ReducerContext) -> u32 {
        match self {
            NodeKindHandle::MatchV1(m) => {
                if let Some(ma) = ctx.db.tab_tm_match().id().find(m) {
                    ma.get_tournament()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::CompetitionV1(c) => {
                if let Some(co) = ctx.db.tab_competition().id().find(c) {
                    co.get_tournament()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::SchedulingV1(sched) => {
                if let Some(ma) = ctx.db.tab_schedule().scheduled_id().find(*sched as u64) {
                    ma.get_tournament()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
        }
    }

    pub(crate) fn split(self) -> (u8, u32) {
        match self {
            NodeKindHandle::MatchV1(m) => (1, m),
            NodeKindHandle::CompetitionV1(c) => (2, c),
            NodeKindHandle::SchedulingV1(s) => (3, s),
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
        }
    }

    pub(crate) fn combine(variant: u8, value: u32) -> Self {
        match variant {
            1 => Self::MatchV1(value),
            2 => Self::CompetitionV1(value),
            3 => Self::SchedulingV1(value),
            _ => unreachable!(),
        }
    }

    /* fn resolve(&self, ctx: &ReducerContext) -> Box<dyn NodeBehaviour> {
        match self {
            NodeKindHandle::MatchV1(m) => ctx.db.tab_tm_match().id().find(m),
            NodeKindHandle::CompetitionV1(_) => todo!(),
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::SchedulingV1(_) => todo!(),
        }
    } */

    /* fn get_unique(self) -> u64 {
        match self {
            NodeKindRef::MatchV1(m) => (1u64 << 32) + m as u64,
            NodeKindRef::CompetitionV1(_) => (2u64 << 32) + m as u64,
            NodeKindRef::MonitoringV1(_) => todo!(),
            NodeKindRef::ServerV1(_) => todo!(),
            NodeKindRef::SchedulingV1(_) => todo!(),
        }
    } */
}

/// Since we need to check either way if the two thing have the same parent we can omit specifing the competition manually.
#[reducer]
pub fn create_connection(
    ctx: &ReducerContext,
    connection_from: NodeKindHandle,
    connection_to: NodeKindHandle,
    setting: ConnectionSettings,
) -> Result<(), String> {
    let account_id = ctx.get_user()?;

    if connection_from == connection_to {
        return Err("Cannot connect a Node to itself.".into());
    }

    let from_comp = connection_from.get_competition(ctx)?;
    let to_comp = connection_to.get_competition(ctx)?;

    if from_comp != to_comp {
        return Err("Cannot add a connection where nodes are part of different parents!".into());
    }

    //TODO maybe this is not necessary but easier for now
    let tournament_id = connection_from.get_tournament(ctx);

    //TODO FIXME: Detect cycles and reject.

    let (connection_from_variant, connection_from) = connection_from.split();
    let (connection_to_variant, connection_to) = connection_to.split();
    if ctx
        .db
        .tab_competition_connection()
        .connection_exists()
        .filter((
            connection_from_variant,
            connection_to_variant,
            connection_from,
            connection_to,
        ))
        .next()
        .is_some()
    {
        return Err("Parallel edges not allowed.".into());
    };

    let connection = ctx
        .db
        .tab_competition_connection()
        .try_insert(TabCompetitionConnection {
            id: 0,
            tournament_id,
            competition_id: from_comp,
            connection_from,
            connection_to,
            connection_from_variant,
            connection_to_variant,
            connection_settings: setting,
            resolved: false,
        })?;

    //If we insert Data Settings we also need to add a row in the data table.
    match connection.connection_settings {
        ConnectionSettings::Waiting => (),
        ConnectionSettings::Data => {
            ctx.db
                .tab_competition_connection_data()
                .try_insert(CompetitionConnectionData::new(
                    connection.id,
                    connection.competition_id,
                ))?;
        }
    }

    Ok(())
}

#[derive(Debug, SpacetimeType)]
pub struct CompetitionConnection {
    tournament_id: u32,
    competition_id: u32,

    connection_from: NodeKindHandle,
    connection_to: NodeKindHandle,

    connection_settings: ConnectionSettings,
}

#[view(name=competition_connection,public)]
pub fn competition_connection(ctx: &ViewContext) -> Vec<CompetitionConnection> {
    ctx.db
        .tab_competition_connection()
        .competition_id()
        //TODO actually make a view arg to filter not return everything.
        .filter(1u32..u32::MAX)
        .map(|v| CompetitionConnection {
            tournament_id: v.tournament_id,
            competition_id: v.competition_id,
            connection_from: NodeKindHandle::combine(v.connection_from_variant, v.connection_from),
            connection_to: NodeKindHandle::combine(v.connection_to_variant, v.connection_to),
            connection_settings: v.connection_settings,
        })
        .collect()
}

#[reducer]
pub fn internal_graph_resolution_node_finished(
    ctx: &ReducerContext,
    competition_id: u32,
    trigger: NodeKindHandle,
) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err(
            "Graph evaluation can not be invoked manually due to its reactive nature.".into(),
        );
    }

    let affected_connections = ctx
        .db
        .tab_competition_connection()
        .competition_id()
        .filter(competition_id)
        .filter(|c| !c.resolved)
        .map(|t| CompetitionConnection {
            tournament_id: t.tournament_id,
            competition_id: t.competition_id,
            connection_from: NodeKindHandle::combine(t.connection_from_variant, t.connection_from),
            connection_to: NodeKindHandle::combine(t.connection_to_variant, t.connection_to),
            connection_settings: t.connection_settings,
        });

    for affected_connection in affected_connections
        .filter(|n| n.connection_from == trigger)
        .map(|c| c.connection_to)
    {
        //affected_connection.try_start()
        log::warn!("{affected_connection:?}");
    }

    /* .map(|v| (v.connection_from, v.connection_to, v.connection_settings)); */
    //let graph = Graph::<(), ConnectionSettings, Directed>::from_edges(iterable);
    //let graph = Acyclic::try_from_graph(graph).map_err(|e| format!("{e:?}"))?;

    Ok(())
}
