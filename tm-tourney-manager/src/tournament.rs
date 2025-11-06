use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{
    auth::Authorization,
    competition::Competition,
    graph::{CompetitionKind, Competitions},
    tournament::registration::Registration,
};
mod registration;

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tournament,public))]
pub struct Tournament {
    #[auto_inc]
    #[primary_key]
    pub id: u64,

    creator: String,
    owners: Vec<String>,

    #[unique]
    name: String,

    description: String,

    status: TournamentStatus,

    competition: u64,
    //TODO maybe make Registration required and add some kind of "Open" to it
    //That would mean that everyone is free to join.
    registration: Option<Registration>,
}

impl Tournament {
    /* pub fn add_competition(&mut self, competition_id: u64) {
        self.competitions
            .try_add_competition(CompetitionKind::CompetitionV1(competition_id));
    }

    pub fn add_match(&mut self, match_id: u64) {
        self.competitions
            .try_add_competition(CompetitionKind::MatchV1(match_id));
    } */
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum TournamentStatus {
    // API cant query it
    Planning,
    // API is public
    Announced,
    //Optional stage entered after Announced.
    //TODO maybe this should be called registration closed and be scheduled
    Registration,
    // Events have started
    Ongoing,
    // Whole Tournament finshed
    Ended,
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn create_tournament(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let user = ctx.auth()?;
    let tournament = ctx
        .db
        .tournament()
        .try_insert(Tournament {
            id: 0,
            name,
            creator: user,
            status: TournamentStatus::Planning,
            owners: Vec::new(),
            registration: None,
            description: "".into(),
            competition: 0,
        })
        .unwrap();

    let competition = Competition::new();

    Ok(())
}
