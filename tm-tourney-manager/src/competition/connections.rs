use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, view};

use crate::{auth::Authorization, competition::competition, r#match::tm_match};

#[spacetimedb::table(name = tab_competition_connection,index(name=connection_exists,btree(columns=[connection_from_variant,connection_from,connection_to_variant,connection_to])))]
pub struct TabCompetitionConnection {
    #[index(btree)]
    competition_id: u32,

    connection_from: u32,
    connection_to: u32,
    connection_from_variant: u8,
    connection_to_variant: u8,

    connection_settings: ConnectionSettings,
}

#[derive(Debug, SpacetimeType)]
pub struct ConnectionSettings {}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum NodeKindRef {
    MatchV1(u32),
    CompetitionV1(u32),
    MapMonitorV1(u32),
    MonitoringV1(u32),
    ServerV1(u32),
}

impl NodeKindRef {
    fn exists(&self, ctx: &ReducerContext) -> bool {
        match self {
            NodeKindRef::MatchV1(m) => ctx.db.tm_match().id().find(m).is_some(),
            NodeKindRef::CompetitionV1(c) => ctx.db.competition().id().find(c).is_some(),
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

#[reducer]
pub fn create_connection(
    ctx: &ReducerContext,
    competition_id: u32,
    connection_from: NodeKindRef,
    connection_to: NodeKindRef,
) -> Result<(), String> {
    let account_id = ctx.is_user()?;

    let Some(comp) = ctx.db.competition().id().find(competition_id) else {
        return Err("Competition could not be found.".into());
    };

    if !connection_from.exists(ctx) {
        return Err("Origin of connection does not exist.".into());
    }
    if !connection_to.exists(ctx) {
        return Err("Target of connection does not exist.".into());
    }

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
            competition_id,
            connection_from,
            connection_to,
            connection_from_variant,
            connection_to_variant,
            connection_settings: ConnectionSettings {},
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

#[view(name=competition_connection,public)]
pub fn competition_connection(ctx: &ViewContext) -> Vec<CompetitionConnection> {
    let competition_id: u32 = 1;
    ctx.db
        .tab_competition_connection()
        .competition_id()
        .filter(competition_id)
        .map(|v| CompetitionConnection {
            competition_id: v.competition_id,
            connection_from: NodeKindRef::combine(v.connection_from_variant, v.connection_from),
            connection_to: NodeKindRef::combine(v.connection_to_variant, v.connection_to),
            connection_settings: v.connection_settings,
        })
        .collect()
}
