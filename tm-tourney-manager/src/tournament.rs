use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, Timestamp, Uuid,
    ViewContext, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{CompetitionV1, tab_competition},
    tournament::permissions::{TournamentPermissionsV1, tab_tournament_permission},
    user::{tab_user__view, tab_user_identity__view},
};

pub(crate) mod permissions;
mod status_schedule;

/// A tournament is a logical grouping of competitions and also the only way to obtain a competition in the first place.
/// It does not provide functionality in of itself but is responsible for all the metadata.
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tab_tournament))]
pub struct TournamentV1 {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(btree)]
    creator_account_id: Uuid,

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
fn create_tournament(
    ctx: &ReducerContext,
    name: String,
    description: String,
    starting_at: Timestamp,
    ending_at: Timestamp,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let tournament = ctx.db.tab_tournament().try_insert(TournamentV1 {
        id: 0,
        name: name.clone(),
        creator_account_id: user.account_id,
        status: TournamentStatus::Planning,
        description,
        starting_at,
        ending_at,
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
    ctx.tournament_permissions(tournament_id, &user)?
        .permission(TournamentPermissionsV1::TOURNAMENT_EDIT_NAME)
        .check()?;

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

    let current_time = ctx.timestamp;

    if tournament.status != TournamentStatus::Planning {
        // Don't allow modifying starting_at if tournament already started
        if tournament.starting_at != starting_at && current_time >= tournament.starting_at {
            return Err(
                "Cannot modify start date of a tournament that has already started.".into(),
            );
        }

        // Don't allow modifying ending_at if tournament already ended
        if tournament.ending_at != ending_at && current_time >= tournament.ending_at {
            return Err("Cannot modify end date of a tournament that has already ended.".into());
        }
    }

    // Don't allow modifying ending_at to before starting_at
    if ending_at < starting_at {
        return Err("Ending date cannot be before starting date.".into());
    }

    tournament.starting_at = starting_at;
    tournament.ending_at = ending_at;

    // Check if the current status needs to be updated based on the new dates
    if tournament.status == TournamentStatus::Announced && current_time >= starting_at {
        // Announced and starting time passed
        tournament.status = TournamentStatus::Ongoing;

        if current_time >= ending_at {
            // Ending time also passed
            tournament.status = TournamentStatus::Ended;
        }
    } else if tournament.status == TournamentStatus::Ongoing && current_time >= ending_at {
        // Ongoing and ending time passed
        tournament.status = TournamentStatus::Ended;
    }

    tournament = ctx.db.tab_tournament().id().update(tournament);

    // Schedule the next status change if applicable
    status_schedule::schedule_tournament_status_change(ctx, &tournament)?;

    Ok(())
}

#[spacetimedb::reducer]
fn tournament_update_status(ctx: &ReducerContext, tournament_id: u32) -> Result<(), String> {
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

    creator_account_id: Uuid,
    creator_name: String,

    name: String,

    starting_at: Timestamp,
    ending_at: Timestamp,

    description: String,

    status: TournamentStatus,
}

#[view(name=my_tournament,public)]
pub fn my_tournament(ctx: &ViewContext) -> Vec<MyTournamentV1> {
    let id = if let Some(user) = ctx.db.tab_user_identity().identity().find(ctx.sender) {
        user.account_id
    } else {
        return Vec::new();
    };

    let Some(user) = ctx.db.tab_user().account_id().find(id) else {
        return Vec::new();
    };

    ctx.db
        .tab_tournament()
        .creator_account_id()
        .filter(id)
        .map(|t| MyTournamentV1 {
            id: t.id,
            creator_account_id: t.creator_account_id,
            creator_name: user.get_name().to_string(),
            name: t.name,
            starting_at: t.starting_at,
            ending_at: t.ending_at,
            description: t.description,
            status: t.status,
        })
        .collect()
}
