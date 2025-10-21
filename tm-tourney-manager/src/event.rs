use spacetimedb::{ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp, reducer, table};
use tm_server_types::event::Event;

use crate::tournament::tournament;

mod scheduling;

#[cfg_attr(feature = "spacetime",spacetimedb::table(name = tournament_event,public))]
pub struct TournamentEvent {
    #[auto_inc]
    #[primary_key]
    pub id: u64,

    tournament: u64,

    // Unique event name for the tournament
    name: String,
    // This could allow eventually to distinguish between monitoring leaderboards and matches or smth.
    //event_type: EventType,
    phase: EventPhase,
    // The Timestamp at which the event starts.
    // If no starting time is selected it has to be started manually.
    starting_at: Timestamp,
    // Estimated duration how long the tourney is gonna take.
    estimate: Option<TimeDuration>,

    // Can capture a server at the end of the registration to serve
    // as a lobby server which automatically delegates players to their
    // corresponding match server.
    // lobby: Option<u64>,

    //registration: Vec<Registration>,
    //generate: Generator,
    //config: EventConfig,

    // Stages get executed sequentially.
    stages: Vec<u64>,
}

impl TournamentEvent {
    pub fn add_stage(&mut self, stage: u64) {
        self.stages.push(stage);
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
    registration: Option<TimeDuration>,
}

/// Adds a new Event to the specified Tournament.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn add_event(
    ctx: &ReducerContext,
    name: String,
    at: Timestamp,
    to: u64,
    with_config: Option<u64>,
) {
    //TODO authorization
    if let Some(mut tournamet) = ctx.db.tournament().id().find(to) {
        let event = ctx.db.tournament_event().insert(TournamentEvent {
            id: 0,
            tournament: to,
            name,
            phase: EventPhase::Planning,
            stages: Vec::new(),
            starting_at: at,
            estimate: None,
            /* config: EventConfig {
                id: 0,
                owner: (),
                public: (),
                name: (),
                registration: (),
            }, */
        });

        tournamet.add_event(event.id);

        ctx.db.tournament().id().update(tournamet);
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_event_template(ctx: &ReducerContext, name: String /* config:  */) {}
