use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, view};

use crate::{auth::Authorization, competition::tab_competition, r#match::tm_match};

#[spacetimedb::table(name = tab_competition_connection,index(name=connection_exists,btree(columns=[connection_from_variant,connection_from,connection_to_variant,connection_to])))]
pub struct TabCompetitionConnection {
    #[index(btree)]
    competition_id: u32,

    tournament_id: u32,

    connection_from: u32,
    connection_to: u32,
    connection_from_variant: u8,
    connection_to_variant: u8,

    connection_settings: ConnectionSettings,
}

#[derive(Debug, SpacetimeType)]
pub enum ConnectionSettings {
    Waiting,
    Data(DataConnectionSettings),
}

/// Very much a placeholder at the moment.
#[derive(Debug, SpacetimeType)]
pub struct DataConnectionSettings {
    count_top: Option<u8>,
    count_bottom: Option<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum NodeKindRef {
    MatchV1(u32),
    CompetitionV1(u32),
    MapMonitorV1(u32),
    MonitoringV1(u32),
    ServerV1(u32),
}

impl NodeKindRef {
    fn get_competition(&self, ctx: &ReducerContext) -> Result<u32, String> {
        match self {
            NodeKindRef::MatchV1(m) => {
                if let Some(ma) = ctx.db.tm_match().id().find(m) {
                    Ok(ma.get_comp_id())
                } else {
                    Err("Origin of connection does not exist.".into())
                }
            }
            NodeKindRef::CompetitionV1(c) => {
                if let Some(co) = ctx.db.tab_competition().id().find(c) {
                    if let Some(id) = co.get_comp_id() {
                        Ok(id)
                    } else {
                        Err("Compeittion without Parent cannot be part of a connection".into())
                    }
                } else {
                    Err("Target of connection does not exist.".into())
                }
            }
            NodeKindRef::MapMonitorV1(_) => todo!(),
            NodeKindRef::MonitoringV1(_) => todo!(),
            NodeKindRef::ServerV1(_) => todo!(),
        }
    }

    fn get_tournament(&self, ctx: &ReducerContext) -> u32 {
        match self {
            NodeKindRef::MatchV1(m) => {
                if let Some(ma) = ctx.db.tm_match().id().find(m) {
                    ma.get_tournament()
                } else {
                    u32::MAX
                }
            }
            NodeKindRef::CompetitionV1(c) => {
                if let Some(co) = ctx.db.tab_competition().id().find(c) {
                    co.get_tournament()
                } else {
                    u32::MAX
                }
            }
            NodeKindRef::MapMonitorV1(_) => todo!(),
            NodeKindRef::MonitoringV1(_) => todo!(),
            NodeKindRef::ServerV1(_) => todo!(),
        }
    }

    fn split(self) -> (u8, u32) {
        match self {
            NodeKindRef::MatchV1(m) => (1, m),
            NodeKindRef::CompetitionV1(c) => (2, c),
            NodeKindRef::MapMonitorV1(_) => todo!(),
            NodeKindRef::MonitoringV1(_) => todo!(),
            NodeKindRef::ServerV1(_) => todo!(),
        }
    }

    fn combine(variant: u8, value: u32) -> Self {
        match variant {
            1 => Self::MatchV1(value),
            2 => Self::CompetitionV1(value),
            _ => unreachable!(),
        }
    }
}

/// Since we need to check either way if the two thing have the same parent we can omit specifing the competition manually.
#[reducer]
pub fn create_connection(
    ctx: &ReducerContext,
    connection_from: NodeKindRef,
    connection_to: NodeKindRef,
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
            connection_from,
            connection_to_variant,
            connection_to,
        ))
        .next()
        .is_some()
    {
        return Err("Parallel edges not allowed.".into());
    };

    ctx.db
        .tab_competition_connection()
        .try_insert(TabCompetitionConnection {
            tournament_id,
            competition_id: from_comp,
            connection_from,
            connection_to,
            connection_from_variant,
            connection_to_variant,
            connection_settings: ConnectionSettings::Waiting,
        })?;

    Ok(())
}

#[derive(Debug, SpacetimeType)]
pub struct CompetitionConnection {
    competition_id: u32,

    connection_from: NodeKindRef,
    connection_to: NodeKindRef,

    connection_settings: ConnectionSettings,
}

//TODO maybe just use for access control
#[view(name=competition_connection,public)]
pub fn competition_connection(ctx: &ViewContext) -> Vec<CompetitionConnection> {
    let competition_id: u32 = 500000;
    ctx.db
        .tab_competition_connection()
        .competition_id()
        .filter(!competition_id)
        .map(|v| CompetitionConnection {
            competition_id: v.competition_id,
            connection_from: NodeKindRef::combine(v.connection_from_variant, v.connection_from),
            connection_to: NodeKindRef::combine(v.connection_to_variant, v.connection_to),
            connection_settings: v.connection_settings,
        })
        .collect()
}
