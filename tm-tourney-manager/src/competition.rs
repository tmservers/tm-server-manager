use spacetimedb::{AnonymousViewContext, Query, ReducerContext, Table, Timestamp, view};

use crate::{auth::Authorization, registration::RegistrationSettings, scheduling::Scheduling};

pub mod connection;
mod scheduling;

/// Always
#[cfg_attr(feature = "spacetime",spacetimedb::table(name = tab_competition))]
pub struct CompetitionV1 {
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
    starting_at: Option<Timestamp>,
    ending_at: Option<Timestamp>,

    scheduling: Scheduling,

    registration_settings: RegistrationSettings,
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
    //entry_points: Option<Vec<NodeIndex>>,
    //competitions: Competitions,
}

impl CompetitionV1 {
    /* pub fn add_competition(&mut self, competition_id: u32) {
        //TODO
        self.competitions
            .try_add_competition(CompetitionKindRef::CompetitionV1(competition_id));
    } */

    /* pub fn add_match(&mut self, match_id: u32) {
        //TODO
        self.competitions
            .try_add_competition(CompetitionKindRef::MatchV1(match_id));
    } */

    pub(crate) fn get_tournament(&self) -> u32 {
        self.tournament_id
    }

    pub(crate) fn get_comp_id(&self) -> Option<u32> {
        self.parent_id
    }

    pub(crate) fn get_name(&self) -> &String {
        &self.name
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
            starting_at: None,
            ending_at: None,
            scheduling: Scheduling::Manual,
            registration_settings: RegistrationSettings::None,
        }
    }

    pub(crate) fn registration_settings(&self) -> &RegistrationSettings {
        &self.registration_settings
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum CompetitionStatus {
    /// If you just created the competition it will be in the planning phase.
    /// Here you can set everything up as you like.
    /// The competition is not visible to the public.
    Planning,
    Registration,
    /// Once the event is ongoing the configuration is immutable.
    /// That means it will play through the configured stages and advancing logic.
    Ongoing,
    /// The whole competition is now immutable.
    Completed,
}

/// Adds a new Event to the specified Tournament.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_competition(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    with_template: Option<u32>,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    // If parent is valid it is guaranteed that it has a valid tournament associated with it.
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid parent_id".into());
    };

    //SAFETY: The competition gets commnited afterwards.
    let new_competition =
        unsafe { CompetitionV1::new(name, Some(parent_id), parent_competition.get_tournament()) };
    ctx.db.tab_competition().try_insert(new_competition)?;

    Ok(())
}

#[view(name=competition,public)]
pub fn competition(ctx: &AnonymousViewContext) -> Query<CompetitionV1> {
    ctx.from
        .tab_competition()
        //TODO this equality doesnt work atm because of enum
        //.r#where(|t| t.status.ne(TournamentStatus::Planning))
        .build()
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_event_template(ctx: &ReducerContext, name: String /* config:  */) {}
