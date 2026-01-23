use spacetimedb::{reducer, view, AnonymousViewContext, Query, ReducerContext, Table, Timestamp};

use crate::{authorization::Authorization, registration::RegistrationSettings};

pub mod connection;

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

    // The two timestamps to display players in which time range the competiion is taking place.
    starting_at: Option<Timestamp>,
    ending_at: Option<Timestamp>,

    registration_settings: RegistrationSettings,
    // TODO Can capture a server at the end of the registration to serve
    // as a lobby server which automatically delegates players to their
    // corresponding desination server based on active matches.
    //lobby: Option<u32>,
}

impl CompetitionV1 {
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
    /// Once the competition is ongoing the configuration is immutable.
    /// That means it will play through the configured stages and advancing logic.
    Ongoing,
    /// The whole competition is now immutable.
    Completed,
}

/// Adds a new Competition to the specified Tournament.
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

#[reducer]
pub fn competition_edit_name(
    ctx: &ReducerContext,
    competition_id: u32,
    name: String,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    competition.name = name;

    ctx.db.tab_competition().id().update(competition);

    Ok(())
}

#[reducer]
pub fn competition_registration_settings(
    ctx: &ReducerContext,
    competition_id: u32,
    registration_settings: RegistrationSettings,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    // If parent is valid it is guaranteed that it has a valid tournament associated with it.
    let Some(mut competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    competition.registration_settings = registration_settings;

    ctx.db.tab_competition().id().update(competition);

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
