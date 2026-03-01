use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table,
    view,
};

use crate::{
    authorization::Authorization, project::permissions::ProjectPermissionsV1,
    registration::RegistrationSettings,
};

pub mod connection;

/// Always
#[table(accessor= tab_competition)]
pub struct CompetitionV1 {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    project_id: u32,
    parent_id: u32,

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
    pub(crate) fn get_project(&self) -> u32 {
        self.project_id
    }

    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn get_name(&self) -> &String {
        &self.name
    }

    /// # Safety
    /// The new competition has to be commited to spacetime db through the `create_competition` reducer.
    /// Otherwise the id is invalid.
    pub unsafe fn new(name: String, parent_id: u32, project_id: u32) -> Self {
        Self {
            id: 0,
            project_id: project_id,
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

#[derive(Debug, SpacetimeType)]
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

/// Adds a new Competition to the specified project.
#[reducer]
pub fn competition_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    with_template: u32,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;

    // If parent is valid it is guaranteed that it has a valid project associated with it.
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid parent_id".into());
    };

    ctx.auth_builder(parent_competition.project_id, account_id)?
        .permission(ProjectPermissionsV1::COMPETITION_CREATE)
        .authorize()?;

    //SAFETY: The competition gets commnited afterwards.
    let new_competition =
        unsafe { CompetitionV1::new(name, parent_id, parent_competition.get_project()) };
    ctx.db.tab_competition().try_insert(new_competition)?;

    Ok(())
}

#[reducer]
pub fn competition_edit_name(
    ctx: &ReducerContext,
    competition_id: u32,
    name: String,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;

    let Some(mut competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(competition.project_id, account_id)?
        .permission(ProjectPermissionsV1::COMPETITION_EDIT_NAME)
        .authorize()?;

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

    // If parent is valid it is guaranteed that it has a valid project associated with it.
    let Some(mut competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(competition.project_id, user.account_id)?
        .permission(ProjectPermissionsV1::COMPETITION_EDIT_REGISTRATION)
        .authorize()?;

    competition.registration_settings = registration_settings;

    ctx.db.tab_competition().id().update(competition);

    Ok(())
}

#[view(accessor=competition,public)]
pub fn competition(ctx: &AnonymousViewContext) -> impl Query<CompetitionV1> {
    ctx.from
        .tab_competition()
        //TODO this equality doesnt work atm because of enum
        //.r#where(|t| t.status.ne(projectStatus::Planning))
        .build()
}
