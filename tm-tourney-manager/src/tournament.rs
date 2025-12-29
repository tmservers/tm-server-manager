use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, Timestamp, ViewContext,
    reducer, table, view,
};

use crate::{
    auth::Authorization,
    competition::{CompetitionV1, tab_competition},
    user::{tab_user__view, user_identity__view},
};

mod status_schedule;

/// A tournament is a logical grouping of competitions and also the only way to obtain a competition in the first place.
/// It does not provide functionality in of itself but is responsible for all the metadata.
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tab_tournament))]
pub struct TournamentV1 {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(btree)]
    creator: String,

    #[unique]
    name: String,

    starting_at: Timestamp,
    ending_at: Timestamp,

    description: String,

    status: TournamentStatus,
}

impl TournamentV1 {}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum TournamentStatus {
    // public API cant query it
    Planning,
    // API is public
    Announced,
    // Competitions have started
    Ongoing,
    // Whole Tournament finshed
    Ended,
}

impl TournamentStatus {
    //TODO this method cannot be used because of custom type
    fn is_public(&self) -> bool {
        *self != TournamentStatus::Planning
    }
}

/// Requires name, description, starting and ending timestamps.
/// Description can be empty.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn create_tournament(ctx: &ReducerContext, name: String, description: String, starting_at: Timestamp, ending_at: Timestamp) -> Result<(), String> {
    let user = ctx.get_user()?;

    let tournament = ctx.db.tab_tournament().try_insert(TournamentV1 {
        id: 0,
        name: name.clone(),
        creator: user,
        status: TournamentStatus::Planning,
        description: description,
        starting_at: starting_at,
        ending_at: ending_at,
    })?;

    //SAFETY: Comitted afterwards
    let competition = unsafe { CompetitionV1::new(name, None, tournament.id) };
    ctx.db.tab_competition().try_insert(competition)?;

    Ok(())
}

#[spacetimedb::reducer]
fn tournament_edit_name(
    ctx: &ReducerContext,
    tournament_id: u32,
    name: String,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut tournament) = ctx.db.tab_tournament().id().find(tournament_id) else {
        return Err("Supplied tournament_id incorrect.".into());
    };

    tournament.name = name;

    ctx.db.tab_tournament().id().update(tournament);

    Ok(())
}

#[spacetimedb::reducer]
fn tournament_edit_description(
    ctx: &ReducerContext,
    tournament_id: u32,
    description: String,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut tournament) = ctx.db.tab_tournament().id().find(tournament_id) else {
        return Err("Supplied tournament_id incorrect.".into());
    };

    tournament.description = description;

    ctx.db.tab_tournament().id().update(tournament);

    Ok(())
}

#[spacetimedb::reducer]
fn tournament_edit_dates(
    ctx: &ReducerContext,
    tournament_id: u32,
    starting_at: Timestamp,
    ending_at: Timestamp,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut tournament) = ctx.db.tab_tournament().id().find(tournament_id) else {
        return Err("Supplied tournament_id incorrect.".into());
    };

    tournament.starting_at = starting_at;
    tournament.ending_at = ending_at;

    // Check if the current status needs to be updated based on the new dates
    let current_time = ctx.timestamp;
    if tournament.status == TournamentStatus::Announced && current_time >= starting_at { // Announced and starting time passed -> Ongoing
        tournament.status = TournamentStatus::Ongoing;
    } else if tournament.status == TournamentStatus::Ongoing && current_time >= ending_at { // Ongoing and ending time passed -> Ended
        tournament.status = TournamentStatus::Ended;
    } else if tournament.status == TournamentStatus::Ongoing && current_time < starting_at { // Ongoing but starting time is now in the future -> Announced
        tournament.status = TournamentStatus::Announced;
    } else if tournament.status == TournamentStatus::Ended && current_time < ending_at { // Ended but ending time is now in the future -> Ongoing
        tournament.status = TournamentStatus::Ongoing;
    }

    tournament = ctx.db.tab_tournament().id().update(tournament);

    // Schedule the next status change if applicable
    status_schedule::schedule_tournament_status_change(ctx, &tournament)?;

    Ok(())
}

#[spacetimedb::reducer]
fn tournament_update_status(
    ctx: &ReducerContext,
    tournament_id: u32
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut tournament) = ctx.db.tab_tournament().id().find(tournament_id) else {
        return Err("Supplied tournament_id incorrect.".into());
    };

    if tournament.status != TournamentStatus::Planning {
        return Err("Tournament status can only be updated from Planning state.".into());
    }

    let current_time = ctx.timestamp;

    if current_time < tournament.starting_at {
        tournament.status = TournamentStatus::Announced;
    } else if current_time >= tournament.starting_at && current_time < tournament.ending_at {
        tournament.status = TournamentStatus::Ongoing;
    } else {
        tournament.status = TournamentStatus::Ended;
    }

    tournament = ctx.db.tab_tournament().id().update(tournament);

    // Schedule the next status change
    status_schedule::schedule_tournament_status_change(ctx, &tournament)?;

    Ok(())
}

#[view(name=tournament,public)]
pub fn tournament(ctx: &AnonymousViewContext) -> Query<TournamentV1> {
    ctx.from
        .tab_tournament()
        //TODO this equality doesnt work atm because of enum
        //.r#where(|t| t.status.ne(TournamentStatus::Planning))
        .build()
}

#[derive(Debug, SpacetimeType)]
pub struct MyTournamentV1 {
    id: u32,

    creator: String,
    creator_name: String,

    name: String,

    starting_at: Timestamp,
    ending_at: Timestamp,

    description: String,

    status: TournamentStatus,
}

#[view(name=my_tournament,public)]
pub fn my_tournament(ctx: &ViewContext) -> Vec<MyTournamentV1> {
    let id = if let Some(user) = ctx.db.user_identity().identity().find(ctx.sender) {
        user.account_id
    } else {
        String::new()
    };

    let Some(user) = ctx.db.tab_user().account_id().find(&id) else {
        return Vec::new();
    };

    ctx.db
        .tab_tournament()
        .creator()
        .filter(&id)
        .map(|t| MyTournamentV1 {
            id: t.id,
            creator: t.creator,
            creator_name: user.get_name().to_string(),
            name: t.name,
            starting_at: t.starting_at,
            ending_at: t.ending_at,
            description: t.description,
            status: t.status,
        })
        .collect()
}
