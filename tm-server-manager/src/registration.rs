use spacetimedb::{ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, tab_competition},
    registration::player::tab_registeration_player,
};

pub mod player;
mod team;
mod template;

#[derive(Debug, SpacetimeType)]
pub enum RegistrationSettings {
    Player(RegistrationSettingsPlayer),
    //TODO
    //Team(RegistrationSettingsTeam),
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

#[table(accessor=tab_registration)]
pub struct Registration {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    settings: RegistrationSettings,

    status: RegistrationStatus,

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

    pub(crate) fn player_registration_allowed(&self, ctx: &ReducerContext) -> Result<(), String> {
        if self.template {
            return Err("Cannot register for a template.".into());
        }
        if self.status != RegistrationStatus::Ongoing {
            return Err("Registration is not ongoing.".into());
        }
        match &self.settings {
            RegistrationSettings::Player(registration_settings_player) => {
                if ctx
                    .db
                    .tab_registeration_player()
                    .registration_id()
                    .filter(self.id)
                    .count()
                    < registration_settings_player.player_limit as usize
                {
                    Ok(())
                } else {
                    Err("Registration maximum players exceeded.".into())
                }
            } /* RegistrationSettings::Team(_) => {
                  Err("Tried to register as a player but it is a team registration.".into())
              } */
        }
    }

    pub(crate) fn team_registration_allowed(&self, ctx: &ReducerContext) -> bool {
        /* self.state == RegistrationState::Ongoing
        && !self.template
        && match &self.settings {
            RegistrationSettings::Player(registration_settings_player) => {
                ctx.db
                    .tab_registered_player()
                    .registration_id()
                    .filter(self.id)
                    .count()
                    < registration_settings_player.player_limit as usize
            }
            RegistrationSettings::Team(_) => false,
        } */
        todo!()
    }

    pub(crate) fn can_change_settings(&self) -> Result<(), String> {
        if !self.status.before_live() {
            return Err("Cannot change registration settings.".into());
        }

        Ok(())
    }
}

#[derive(Debug, SpacetimeType, PartialEq, Eq)]
enum RegistrationStatus {
    Configuring,
    Upcoming,
    Ongoing,
    Ended,
    Locked,
}

impl RegistrationStatus {
    fn before_live(&self) -> bool {
        match self {
            RegistrationStatus::Configuring => true,
            RegistrationStatus::Upcoming => true,
            RegistrationStatus::Ongoing => false,
            RegistrationStatus::Ended => false,
            RegistrationStatus::Locked => false,
        }
    }
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

    if ctx
        .db
        .tab_competition()
        .id()
        .find(parent_id)
        .unwrap()
        .is_template()
    {
        return Err("Cannot add a normal node to a template".into());
    };
    if with_template != 0 {
        let Some(template) = ctx.db.tab_registration().id().find(with_template) else {
            return Err("Template not found!".into());
        };
        //TODO do we have access to this template?
        let new_registration = template.instantiate(parent_id, false);
        ctx.db.tab_registration().try_insert(new_registration)?;
    } else {
        ctx.db.tab_registration().try_insert(Registration {
            name,
            id: 0,
            parent_id,
            settings: RegistrationSettings::Player(RegistrationSettingsPlayer {
                player_limit: 100,
            }),
            status: RegistrationStatus::Configuring,
            template: false,
        })?;
    }

    Ok(())
}

//TODO codegen bug
/* #[reducer]
fn registration_settings(
    ctx: &ReducerContext,
    id: u32,
    settings: RegistrationSettings,
) -> Result<(), String> {
    let Some(mut registration) = ctx.db.tab_registration().id().find(id) else {
        return Err("Registration not found.".into());
    };

    ctx.auth_builder(registration.parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    registration.can_change_settings()?;

    registration.settings = settings;

    ctx.db.tab_registration().id().update(registration);

    Ok(())
}
 */

#[reducer]
fn registration_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut registration) = ctx.db.tab_registration().id().find(id) else {
        return Err("Registration not found.".into());
    };

    ctx.auth_builder(registration.parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    registration.status = RegistrationStatus::Upcoming;

    ctx.db.tab_registration().id().update(registration);

    Ok(())
}

#[reducer]
fn registration_start(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut registration) = ctx.db.tab_registration().id().find(id) else {
        return Err("Registration not found.".into());
    };

    ctx.auth_builder(registration.parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    registration.status = RegistrationStatus::Ongoing;

    ctx.db.tab_registration().id().update(registration);

    Ok(())
}

#[reducer]
fn registration_end(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut registration) = ctx.db.tab_registration().id().find(id) else {
        return Err("Registration not found.".into());
    };

    ctx.auth_builder(registration.parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    registration.status = RegistrationStatus::Ended;

    ctx.db.tab_registration().id().update(registration);

    Ok(())
}
