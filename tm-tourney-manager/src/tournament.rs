use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{
    auth::Authorization,
    competition::{Competition, competition},
    graph::{CompetitionKind, Competitions},
};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tournament,public))]
pub struct Tournament {
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

impl Tournament {
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

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn create_tournament(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let user = ctx.auth_user()?;
    let mut tournament = ctx
        .db
        .tournament()
        .try_insert(Tournament {
            id: 0,
            name: name.clone(),
            creator: user,
            status: TournamentStatus::Planning,
            owners: Vec::new(),
            description: "".into(),
            competition: 0,
        })
        .unwrap();

    //SAFETY: Comitted afterwards
    let competition = unsafe { Competition::new(name, None, tournament.id) };
    let competition = ctx.db.competition().try_insert(competition)?;

    tournament.set_competition(competition.id);
    ctx.db.tournament().id().update(tournament);

    Ok(())
}
