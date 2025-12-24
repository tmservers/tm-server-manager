use spacetimedb::{ReducerContext, Table, Timestamp, reducer, table};

use crate::{auth::Authorization, competition::tab_competition};

#[table(name= registration_player)]
pub struct RegistrationPlayer {
    competition: u32,
    player_id: String,
    registered_at: Timestamp,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum RegistrationRules {
    Players(RegistrationPlayerRules),
    Team(RegistrationTeamRules),
    Inherit,
    Open,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RegistrationPlayerRules {
    player_limit: Option<u32>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RegistrationTeamRules {
    team_limit: Option<u32>,
    team_size_min: u8,
    team_size_max: u8,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct TeamInfo {
    registered_at: Timestamp,
    name: String,
    creator: String,
    members: Vec<String>,
}

#[reducer]
pub fn competition_register_player(ctx: &ReducerContext, compeition_id: u32) -> Result<(), String> {
    let player_id = ctx.get_user()?;

    let Some(comp) = ctx.db.tab_competition().id().find(compeition_id) else {
        return Err("Competition not found".into());
    };
    let rules = comp.registration_rules();

    //rules.

    let player_count = ctx
        .db
        .registration_player()
        .iter()
        .filter(|d| d.competition == compeition_id)
        .count();
    let Some(comp) = ctx.db.tab_competition().id().find(compeition_id) else {
        return Err(format!(
            "Competition with id {compeition_id} was not found."
        ));
    };

    Ok(())
}

#[reducer]
pub fn competition_unregister_player(
    ctx: &ReducerContext,
    compeition_id: u32,
) -> Result<(), String> {
    ctx.get_user()?;

    let Some(comp) = ctx.db.tab_competition().id().find(compeition_id) else {
        return Err(format!(
            "Competition with id {compeition_id} was not found."
        ));
    };

    Ok(())
}
