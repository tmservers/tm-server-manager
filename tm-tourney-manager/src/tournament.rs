use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    auth::Authorization,
    competition::{Competition, competition},
};

/// A tournament is a logical grouping of competitions and also the only way to obtain a competition in the first place.
/// It does not provide functionality in of itself but is responsible for all the metadata.
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tounrament,public))] //TODO make private and rename use view instead
pub struct TabTournament {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    creator: String,
    owners: Vec<String>,

    #[unique]
    name: String,

    description: String,

    status: TournamentStatus,

    competition: u32,
}

impl TabTournament {
    pub(crate) fn set_competition(&mut self, comp_id: u32) {
        self.competition = comp_id
    }
}

#[derive(Debug)]
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

/// The only thing necessary for a creation of a tounrnant is a unique name.
/// The rest of the setup can must be made in subsequent calls.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn create_tournament(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let user = ctx.auth_user()?;

    let mut tournament = ctx.db.tounrament().try_insert(TabTournament {
        id: 0,
        name: name.clone(),
        creator: user,
        status: TournamentStatus::Planning,
        owners: Vec::new(),
        description: "".into(),
        competition: 0,
    })?;

    //SAFETY: Comitted afterwards
    let competition = unsafe { Competition::new(name, None, tournament.id) };
    let competition = ctx.db.competition().try_insert(competition)?;

    tournament.set_competition(competition.id);
    ctx.db.tounrament().id().update(tournament);

    Ok(())
}

/// The exposed type to receive tournaments.
#[derive(Debug, SpacetimeType)]
pub struct TournamentV1 {}

/* #[view(name=tournament,public)]
pub fn tournament(ctx: &ViewContext) -> Vec<TournamentV1> {
    ctx.db.tab_tournament().id().find(1).unwrap()]
} */
