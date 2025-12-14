use spacetimedb::{ReducerContext, Table, TimeDuration};

use crate::{
    auth::Authorization,
    graph::{CompetitionKind, Competitions, NodeIndex},
    registration::RegistrationRules,
    scheduling::Scheduling,
    tournament::tab_tournament,
};

mod scheduling;

#[cfg_attr(feature = "spacetime",spacetimedb::table(name = competition,public))]
pub struct Competition {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    tournament_id: u32,
    parent_id: Option<u32>,

    name: String,

    // Necessary to hide and mark as immutable
    status: CompetitionStatus,
    // The Timestamp at which the event starts.
    // If no starting time is selected it has to be started manually.
    //starting_at: Timestamp,
    // Estimated duration how long the competition is gonna take.
    estimate: Option<TimeDuration>,

    scheduling: Scheduling,

    registration_rules: RegistrationRules,

    // TODO Can capture a server at the end of the registration to serve
    // as a lobby server which automatically delegates players to their
    // corresponding desination server based on active matches.
    //lobby: Option<u32>,

    //TODO the configured generator can spit out nodes for the competition.
    //generator: Generator,

    //TODO something along the lines of this
    //is necessary to delegate things in the following
    //recursion level.
    //This could be triggered with a schedule but an alternatie
    // approach would be to save the affected entry points in a schedule
    // This would be possible since they are own rows in the first place.
    // We could also have some sort of a barrier table or smth which takes care of this.
    entry_points: Option<Vec<NodeIndex>>,

    competitions: Competitions,
}

impl Competition {
    pub fn add_competition(&mut self, competition_id: u32) {
        //TODO
        self.competitions
            .try_add_competition(CompetitionKind::CompetitionV1(competition_id));
    }

    pub fn add_match(&mut self, match_id: u32) {
        //TODO
        self.competitions
            .try_add_competition(CompetitionKind::MatchV1(match_id));
    }

    pub(crate) fn get_tournament(&self) -> u32 {
        self.tournament_id
    }

    /// # Safety
    /// The new competition has to be commited to spacetime db through the `create_competition` reducer.
    /// Otherwise the id is invalid.
    pub unsafe fn new(name: String, parent_id: Option<u32>, tournament_id: u32) -> Self {
        Self {
            id: 0,
            tournament_id,
            parent_id,
            name,
            status: CompetitionStatus::Planning,
            estimate: None,
            competitions: Competitions::new(),
            entry_points: None,
            scheduling: Scheduling::Manual,
            registration_rules: RegistrationRules::Open,
        }
    }

    pub(crate) fn registration_rules(&self) -> &RegistrationRules {
        &self.registration_rules
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum CompetitionStatus {
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
    /// The whole competition is now immutable.
    Completed,
}

#[cfg_attr(feature="spacetime",spacetimedb::table(name = event_config,public))]
pub struct EventConfig {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    #[cfg_attr(feature = "spacetime", primary_key)]
    id: u32,

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
    parent_id: u32,
    with_template: Option<u32>,
) -> Result<(), String> {
    let user = ctx.auth_user()?;

    // If parent is valid it is guaranteed that it has a valid tournament associated with it.
    let Some(mut parent_competition) = ctx.db.competition().id().find(parent_id) else {
        return Err("Invalid parent_id".into());
    };

    //SAFETY: The competition gets commnited afterwards.
    let new_competition =
        unsafe { Competition::new(name, Some(parent_id), parent_competition.get_tournament()) };
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
pub fn add_dependency(
    ctx: &ReducerContext,
    comp_id: u32,
    from_node: u32,
    to_node: u32,
) -> Result<(), String> {
    ctx.auth_user()?;

    /*   let Some(from_id) = ctx.db.competition().id().find(from_id) else {
        return Err(format!("Competition with id {from_id} not found."));
    };
    let Some(to_id) = ctx.db.competition().id().find(to_id) else {
        return Err(format!("Competition with id {to_id} not found."));
    };

    if from_id.parent_id != to_id.parent_id || from_id.tournament_id != to_id.tournament_id {} */

    Ok(())
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_event_template(ctx: &ReducerContext, name: String /* config:  */) {}
