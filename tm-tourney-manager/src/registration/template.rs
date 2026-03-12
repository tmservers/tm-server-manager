use spacetimedb::{ReducerContext, Table, TimeDuration, reducer};

use crate::{
    authorization::Authorization,
    competition::CompetitionPermissionsV1,
    registration::{
        Registration, RegistrationDeadline, RegistrationSettings, RegistrationSettingsPlayer,
        RegistrationState, tab_registration,
    },
};

#[reducer]
fn registration_template_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
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
        template: true,
        // 3.47 Days of relate duration.
        deadline: RegistrationDeadline::Relative(TimeDuration::from_micros(300000000000)),
    })?;

    Ok(())
}
