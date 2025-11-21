use spacetimedb::{ReducerContext, Timestamp, reducer};

use crate::competition::competition;

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum Registration {
    Players(PlayerRegistration),
    Team(TeamRegistration),
    Inherit,
    Open,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct PlayerRegistration {
    player_limit: Option<u32>,
    players: Vec<String>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct TeamRegistration {
    team_limit: Option<u32>,
    team_size_min: u8,
    team_size_max: u8,
    teams: Vec<TeamInfo>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct TeamInfo {
    registered_at: Timestamp,
    name: String,
    members: Vec<String>,
}

#[reducer]
pub fn competition_register(ctx: &ReducerContext, compeition_id: u32) -> Result<(), String> {
    let Some(comp) = ctx.db.competition().id().find(compeition_id) else {
        return Err(format!(
            "Competition with id {compeition_id} was not found."
        ));
    };

    Ok(())
}

#[reducer]
pub fn competition_unregister(ctx: &ReducerContext, compeition_id: u32) -> Result<(), String> {
    let Some(comp) = ctx.db.competition().id().find(compeition_id) else {
        return Err(format!(
            "Competition with id {compeition_id} was not found."
        ));
    };

    Ok(())
}
