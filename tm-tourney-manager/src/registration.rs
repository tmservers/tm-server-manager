use spacetimedb::{ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, tab_competition},
};

mod player;
mod team;
mod template;

#[derive(Debug, SpacetimeType)]
pub enum RegistrationSettings {
    Player(RegistrationSettingsPlayer),
    Team(RegistrationSettingsTeam),
}

#[derive(Debug, SpacetimeType)]
pub struct RegistrationSettingsPlayer {
    player_limit: u32,
}

#[derive(Debug, SpacetimeType)]
pub struct RegistrationSettingsTeam {
    team_limit: u32,
    team_size_min: u8,
    team_size_max: u8,
}

/* #[derive(Debug, SpacetimeType)]
pub enum RegistrationDeadline {
    Relative(TimeDuration),
    Abosulute(Timestamp),
} */

#[table(accessor=tab_registration)]
pub struct Registration {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    settings: RegistrationSettings,

    //deadline: RegistrationDeadline,
    state: RegistrationState,

    template: bool,
}

impl Registration {
    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn instantiate(mut self, parent_id: u32, stay_template: bool) -> Self {
        self.parent_id = parent_id;
        self.id = 0;
        self.template = stay_template;
        self
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
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    ctx.db.tab_registration().try_insert(Registration {
        name,
        id: 0,
        parent_id,
        settings: RegistrationSettings::Player(RegistrationSettingsPlayer { player_limit: 100 }),
        state: RegistrationState::Configuring,
        template: false,
        // 3.47 Days of relate duration.
        //deadline: RegistrationDeadline::Relative(TimeDuration::from_micros(300000000000)),
    })?;

    Ok(())
}
