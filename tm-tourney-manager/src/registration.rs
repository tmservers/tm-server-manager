use spacetimedb::{ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use crate::{
    authorization::Authorization, competition::tab_competition,
    project::permissions::ProjectPermissionsV1,
};

mod player;
mod team;

#[derive(Debug, SpacetimeType)]
pub enum RegistrationSettings {
    Players(RegistrationPlayerSettings),
    Team(RegistrationTeamSettings),
    None,
}

#[derive(Debug, SpacetimeType)]
pub struct RegistrationPlayerSettings {
    player_limit: Option<u32>,
    registration_deadline: Timestamp,
}

#[derive(Debug, SpacetimeType)]
pub struct RegistrationTeamSettings {
    team_limit: Option<u32>,
    team_size_min: u8,
    team_size_max: u8,
    registration_deadline: Timestamp,
}

#[table(accessor=tab_registration)]
pub struct Registration {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    parent_id: u32,
    project_id: u32,

    settings: RegistrationSettings,

    registration_state: RegistrationState,
}

impl Registration {
    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn get_project(&self) -> u32 {
        self.project_id
    }
}

#[derive(Debug, SpacetimeType)]
enum RegistrationState {
    Configuring,
    Upcoming,
    Ongoing,
    Ended,
    Locked,
}

#[reducer]
fn registration_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    with_template: u32,
) -> Result<(), String> {
    let user = ctx.get_user_account()?;

    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(parent_competition.get_project(), user)?
        .permission(ProjectPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    ctx.db.tab_registration().try_insert(Registration {
        name,
        id: 0,
        parent_id,
        project_id: parent_competition.get_project(),
        settings: RegistrationSettings::None,
        registration_state: RegistrationState::Configuring,
    })?;

    Ok(())
}
