use spacetimedb::{CtxDbRead, ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1, CompetitionRead,
        node::{NodeHandle, NodeWrite, Vec2},
    },
    registration::player::tab_registeration_player,
};

pub mod player;
//mod team;

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

/* #[derive(Debug, SpacetimeType)]
pub struct RegistrationSettingsTeam {
    team_limit: u32,
    team_size_min: u8,
    team_size_max: u8,
} */

//TODO make private again
#[table(accessor=tab_registration,public,vis_private)]
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

    pub(crate) fn name(&self) -> &str {
        &self.name
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

    /* pub(crate) fn team_registration_allowed(&self, ctx: &ReducerContext) -> bool {
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
    } */

    pub(crate) fn can_change_settings(&self) -> Result<(), String> {
        if !self.status.before_ongoing() {
            return Err("Cannot change registration settings.".into());
        }

        Ok(())
    }
}

#[derive(Debug, SpacetimeType, PartialEq, Eq)]
enum RegistrationStatus {
    Configuring,
    Configured,
    Ongoing,
    Ended,
}

impl RegistrationStatus {
    fn before_ongoing(&self) -> bool {
        match self {
            RegistrationStatus::Configuring => true,
            RegistrationStatus::Configured => true,
            RegistrationStatus::Ongoing => false,
            RegistrationStatus::Ended => false,
            //RegistrationStatus::Locked => false,
        }
    }
}

#[reducer]
fn registration_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    position: Vec2,
    with_template: u32,
    as_template: bool,
) -> Result<(), String> {
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    if ctx.competition_find(parent_id)?.is_template() {
        return Err("Cannot add a normal node to a template".into());
    };

    if with_template != 0 {
        todo!()
    } else {
        RegistrationWrite::registration_create(ctx, name, parent_id, position, None, as_template)?;
    }

    Ok(())
}

#[reducer]
fn registration_settings_update(
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

#[reducer]
fn registration_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut registration) = ctx.db.tab_registration().id().find(id) else {
        return Err("Registration not found.".into());
    };

    ctx.auth_builder(registration.parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    if registration.status != RegistrationStatus::Configuring {
        return Err("Is not in configuring state.".into());
    }

    registration.status = RegistrationStatus::Configured;

    ctx.db.tab_registration().id().update(registration);

    Ok(())
}

#[reducer]
fn registration_start(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(registration) = ctx.db.tab_registration().id().find(id) else {
        return Err("Registration not found.".into());
    };

    ctx.auth_builder(registration.parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    ctx.registration_open(id)
}

#[reducer]
fn registration_end(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut registration) = ctx.db.tab_registration().id().find(id) else {
        return Err("Registration not found.".into());
    };

    ctx.auth_builder(registration.parent_id)
        .permission(CompetitionPermissionsV1::REGISTRATION_CREATE)
        .authorize()?;

    ctx.registration_close(id)
}

pub(crate) trait RegistrationRead {
    fn registrations_with_competition(
        &self,
        competition_id: u32,
    ) -> impl Iterator<Item = Registration>;
    fn registration_find(&self, id: u32) -> Result<Registration, String>;
}

impl<Db: CtxDbRead> RegistrationRead for Db {
    fn registration_find(&self, id: u32) -> Result<Registration, String> {
        let Some(registration) = self.db_read_only().tab_registration().id().find(id) else {
            return Err("Registration not found!".into());
        };

        Ok(registration)
    }

    fn registrations_with_competition(
        &self,
        competition_id: u32,
    ) -> impl Iterator<Item = Registration> {
        self.db_read_only()
            .tab_registration()
            .parent_id()
            .filter(competition_id)
    }
}

pub(crate) trait RegistrationWrite: RegistrationRead {
    fn registration_name_edit(&self, registration_id: u32, name: String) -> Result<(), String>;
    fn registration_open(&self, registration_id: u32) -> Result<(), String>;
    fn registration_close(&self, registration_id: u32) -> Result<(), String>;
    fn registration_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<Registration>,
        as_template: bool,
    ) -> Result<Registration, String>;
}

impl<Db: spacetimedb::CtxDbWrite> RegistrationWrite for Db {
    fn registration_name_edit(&self, registration_id: u32, name: String) -> Result<(), String> {
        let Some(mut registration) = self.db().tab_registration().id().find(registration_id) else {
            return Err("Match not found.".into());
        };
        registration.name = name;
        self.db().tab_registration().id().update(registration);

        Ok(())
    }

    fn registration_open(&self, registration_id: u32) -> Result<(), String> {
        let Some(mut registration) = self.db().tab_registration().id().find(registration_id) else {
            return Err("Registration not found.".into());
        };

        if registration.is_template() {
            return Err("Cannot be called on templates".into());
        }

        if registration.status != RegistrationStatus::Configured {
            return Err("Is not in Configured state.".into());
        }

        registration.status = RegistrationStatus::Ongoing;

        self.db().tab_registration().id().update(registration);

        Ok(())
    }

    fn registration_close(&self, registration_id: u32) -> Result<(), String> {
        let Some(mut registration) = self.db().tab_registration().id().find(registration_id) else {
            return Err("Registration not found.".into());
        };

        if registration.is_template() {
            return Err("Cannot be called on templates".into());
        }

        if registration.status != RegistrationStatus::Ongoing {
            return Err("Is not in ongoing state.".into());
        }

        registration.status = RegistrationStatus::Ended;

        self.db().tab_registration().id().update(registration);

        Ok(())
    }

    fn registration_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<Registration>,
        as_template: bool,
    ) -> Result<Registration, String> {
        let registration = if let Some(mut template) = template {
            template.template = as_template; //TODO: Audit codepath for legal again.
            template.parent_id = parent_id;
            template.id = 0;
            template
        } else {
            Registration {
                name,
                id: 0,
                parent_id,
                settings: RegistrationSettings::Player(RegistrationSettingsPlayer {
                    player_limit: 100,
                }),
                status: RegistrationStatus::Configuring,
                template: false,
            }
        };

        let registration = self.db().tab_registration().try_insert(registration)?;
        self.node_create(NodeHandle::RegistrationV1(registration.id), position)?;

        Ok(registration)
    }
}

/* #[view(accessor=unstable_registration,public)]
fn unstable_registration(ctx: &AnonymousViewContext) -> impl Query<Registration> {
    ctx.from.tab_registration()
} */
