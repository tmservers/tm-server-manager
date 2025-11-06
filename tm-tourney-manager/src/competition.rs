use spacetimedb::{ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp, reducer, table};
use tm_server_types::event::Event;

use crate::{
    auth::Authorization,
    graph::{CompetitionKind, Competitions, NodeIndex},
    tournament::tournament,
};

mod scheduling;

#[cfg_attr(feature = "spacetime",spacetimedb::table(name = competition,public))]
pub struct Competition {
    #[auto_inc]
    #[primary_key]
    pub id: u64,

    tournament_id: u64,
    parent_id: Option<u64>,

    name: String,
    // This could allow eventually to distinguish between monitoring leaderboards and matches or smth.
    //event_type: EventType,
    phase: EventPhase,
    // The Timestamp at which the event starts.
    // If no starting time is selected it has to be started manually.
    //starting_at: Timestamp,
    // Estimated duration how long the tourney is gonna take.
    estimate: Option<TimeDuration>,

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

    pub unsafe fn new(name: String, parent_id: Option<u64>, tournament_id: u64) -> Self {
        Self {
            id: 0,
            tournament_id,
            parent_id,
            name,
            phase: EventPhase::Planning,
            estimate: None,
            competitions: Competitions::new(),
            entry_points: None,
        }
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
pub fn create_competition(
    ctx: &ReducerContext,
    name: String,
    tournament_id: u64,
    parent_id: u64,
    with_config: Option<u64>,
) -> Result<(), String> {
    let user = ctx.auth()?;

    // Tournament and parent ids need to be valid.
    if ctx.db.tournament().id().find(tournament_id).is_none() {
        return Err("Invalid tournament_id".into());
    };
    let Some(mut parent_competition) = ctx.db.competition().id().find(parent_id) else {
        return Err("Invalid parent_id".into());
    };

    //SAFETY: The competition gets commnited afterwards.
    let new_competition = unsafe { Competition::new(name, Some(parent_id), tournament_id) };
    match ctx.db.competition().try_insert(new_competition) {
        // If the insertion of the new competition succeeds we need to update the parent
        // to include it in the graph.
        Ok(competition) => {
            parent_competition.add_competition(competition.id);
            ctx.db.competition().id().update(parent_competition);
            Ok(())
        }
        Err(err) => Err(err.to_string()),
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_event_template(ctx: &ReducerContext, name: String /* config:  */) {}
