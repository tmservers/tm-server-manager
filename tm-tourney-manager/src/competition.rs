use tm_server_types::event::Event;

use crate::{
    graph::{CompetitionKind, Competitions, NodeIndex},
    tournament::tournament,
};

mod scheduling;

#[cfg_attr(feature = "spacetime",spacetimedb::table(name = competition,public))]
pub struct Competition {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    #[cfg_attr(feature = "spacetime", primary_key)]
    pub id: u64,

    tournament_id: u64,
    parent_id: u64,

    // Unique event name for the tournament
    name: String,
    // This could allow eventually to distinguish between monitoring leaderboards and matches or smth.
    //event_type: EventType,
    phase: EventPhase,
    // The Timestamp at which the event starts.
    // If no starting time is selected it has to be started manually.
    starting_at: spacetimedb::Timestamp,
    // Estimated duration how long the tourney is gonna take.
    estimate: Option<spacetimedb::TimeDuration>,

    // Can capture a server at the end of the registration to serve
    // as a lobby server which automatically delegates players to their
    // corresponding match server.
    // lobby: Option<u64>,

    //registration: Vec<Registration>,
    //generate: Generator,
    //config: EventConfig,

    //TODO something along the lines of this
    //is necessary to delegate things in the following
    //recursion level.
    //This could be triggered with a schedule but an alternatie
    // approach would be to save the affected entry points in a schedule
    // This would be possible since they are own rows in the first place.
    // We could also have some sort of a barrier table or smth which takes care of this.
    entry_points: Option<Vec<NodeIndex>>,

    // Stages get executed sequentially.
    //stages: Vec<u64>,
    competitions: Competitions,
}

impl Competition {
    pub fn add_competition(&mut self, competition_id: u64) {
        //TODO
        self.competitions
            .try_add_competition(CompetitionKind::CompetitionV1(competition_id));
    }

    pub fn add_match(&mut self, match_id: u64) {
        //TODO
        self.competitions
            .try_add_competition(CompetitionKind::MatchV1(match_id));
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum EventPhase {
    /// If you just created the event it will be in the planning phase.
    /// Here you can set everything up as you like.
    /// The event is not visible to the public.
    Planning,
    /// To advance into the preparation phase you MUST define a starting time aswell as
    /// an estimated duration how long the event will take.
    Preparation,
    /// Once the event is ongoing the configuration is immutable.
    /// That means it will play through the configured stages and advancing logic.
    Ongoing,
    /// The whole tournament is now immutable.
    Completed,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum EventType {
    Matches,
    TimeAttack,
}

#[cfg_attr(feature="spacetime",spacetimedb::table(name = event_config,public))]
pub struct EventConfig {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    #[cfg_attr(feature = "spacetime", primary_key)]
    id: u64,

    owner: String,
    public: bool,
    // Global identifier for the event config.
    #[cfg_attr(feature = "spacetime", unique)]
    name: String,

    ///  Determines if the
    registration: Option<spacetimedb::TimeDuration>,
}

/// Adds a new Event to the specified Tournament.
#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_competition(
    ctx: &spacetimedb::ReducerContext,
    name: String,
    at: spacetimedb::Timestamp,
    tournament_id: u64,
    parent_id: u64,
    with_config: Option<u64>,
) {
    use spacetimedb::Table;

    //TODO authorization
    let new_competition = Competition {
        id: 0,
        tournament_id,
        parent_id,
        name,
        phase: EventPhase::Planning,
        starting_at: at,
        estimate: None,
        competitions: Competitions::new(),
        entry_points: None,
    };

    if tournament_id == parent_id {
        if let Some(mut tournament) = ctx.db.tournament().id().find(parent_id) {
            let comp = ctx.db.competition().insert(new_competition);
            tournament.add_match(comp.id);
            ctx.db.tournament().id().update(tournament);
        }
    } else if let Some(mut competition) = ctx.db.competition().id().find(parent_id) {
        let comp = ctx.db.competition().insert(new_competition);
        competition.add_competition(comp.id);
        ctx.db.competition().id().update(competition);
    }
}

#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_event_template(ctx: &spacetimedb::ReducerContext, name: String /* config:  */) {}
