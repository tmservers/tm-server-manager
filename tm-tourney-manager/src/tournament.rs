use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, Timestamp, ViewContext,
    reducer, table, view,
};

use crate::{
    auth::Authorization,
    competition::{CompetitionV1, tab_competition},
    user::{tab_user__view, user_identity__view},
};

/// A tournament is a logical grouping of competitions and also the only way to obtain a competition in the first place.
/// It does not provide functionality in of itself but is responsible for all the metadata.
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tab_tournament))]
pub struct TournamentV1 {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    creator: String,
    owners: Vec<String>,

    #[unique]
    name: String,

    starting_at: Option<Timestamp>,
    ending_at: Option<Timestamp>,

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

/// The only thing necessary for a creation of a tounrnant is a unique name.
/// The rest of the setup can must be made in subsequent calls.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn create_tournament(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let user = ctx.get_user()?;

    let tournament = ctx.db.tab_tournament().try_insert(TournamentV1 {
        id: 0,
        name: name.clone(),
        creator: user,
        status: TournamentStatus::Planning,
        owners: Vec::new(),
        description: "".into(),
        starting_at: None,
        ending_at: None,
    })?;

    //SAFETY: Comitted afterwards
    let competition = unsafe { CompetitionV1::new(name, None, tournament.id) };
    ctx.db.tab_competition().try_insert(competition)?;

    Ok(())
}

#[spacetimedb::reducer]
fn tournament_edit_description(
    ctx: &ReducerContext,
    tounrnament_id: u32,
    description: String,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut tournament) = ctx.db.tab_tournament().id().find(tounrnament_id) else {
        return Err("Supplied tournament_id incorrect.".into());
    };

    tournament.description = description;

    ctx.db.tab_tournament().id().update(tournament);

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

#[view(name=my_tournament,public)]
pub fn my_tournament(ctx: &ViewContext) -> Query<TournamentV1> {
    let id = if let Some(user) = ctx.db.user_identity().identity().find(ctx.sender) {
        user.account_id
    } else {
        String::new()
    };

    ctx.from
        .tab_tournament()
        //TODO more advanced access control with collaborators and stuff.
        .r#where(|t| t.creator.eq(id.clone()))
        .build()
}
