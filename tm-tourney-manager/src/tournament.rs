use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{graph::Competitions, tournament::registration::Registration};
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

    //events: Vec<u64>,
    competitions: Competitions,
    //TODO maybe make Registration required and add some kind of "Open" to it
    //That would mean that everyone is free to join.
    registration: Option<Registration>,
}

impl Tournament {
    pub fn add_competition(&mut self, competition: u64) {
        //TODO
        //self.events.push(competition);
    }
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
fn create_tournament(ctx: &ReducerContext, name: String) {
    //if let Some(user)=ctx.db.user().id ctx.identity()
    //ctx.
    //TODO authorization
    ctx.db.tournament().insert(Tournament {
        name,
        creator: "yomama".into(),
        id: 0,
        status: TournamentStatus::Planning,
        owners: Vec::new(),
        //events: Vec::new(),
        registration: None,
        description: "".into(),
        competitions: Competitions::new(),
    });
}
