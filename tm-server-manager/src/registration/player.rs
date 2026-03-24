use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, Table, Timestamp, Uuid, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    registration::{self, tab_registration},
};

#[table(accessor=tab_registeration_player)]
#[derive(Debug, Clone, Copy)]
pub struct RegisterationPlayer {
    pub account_id: Uuid,
    pub registered_at: Timestamp,
    #[index(hash)]
    pub registration_id: u32,
}

#[view(accessor=temp_registration_player,public)]
pub fn temp_registration_player(
    ctx: &AnonymousViewContext, /* ,registration_id: u32 */
) -> Vec<RegisterationPlayer> {
    let registration_id = 2u32;
    ctx.registration_player(registration_id)
}

#[reducer]
pub fn register_player(ctx: &ReducerContext, registration_id: u32) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Tried to register but the registration id does not exist.".into());
    };

    registration.player_registration_allowed(ctx)?;

    if ctx
        .db
        .tab_registeration_player()
        .registration_id()
        .filter(registration_id)
        .any(|p| p.account_id == user.account_id)
    {
        return Err("User is already registered for registration_id!".to_string());
    }

    ctx.db
        .tab_registeration_player()
        .try_insert(RegisterationPlayer {
            registration_id,
            account_id: user.account_id,
            registered_at: ctx.timestamp,
        })?;

    Ok(())
}

#[reducer]
pub fn unregister_player(ctx: &ReducerContext, registration_id: u32) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;

    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Tried to register for a competition that doesnt exist.".into());
    };

    registration.player_registration_allowed(ctx)?;

    let Some(registred_user) = ctx
        .db
        .tab_registeration_player()
        .registration_id()
        .filter(registration_id)
        .find(|p| p.account_id == account_id)
    else {
        return Err("User is already registered for competition!".to_string());
    };

    if !ctx.db.tab_registeration_player().delete(registred_user) {
        return Err(format!(
            "Unexpected error occured deleting the user {} from {}",
            account_id, registration_id
        ));
    };

    Ok(())
}

pub(crate) trait RegistrationRead {
    fn registration_player(&self, registration_id: u32) -> Vec<RegisterationPlayer>;
}
impl<Db: spacetimedb::DbContext> RegistrationRead for Db {
    fn registration_player(&self, registration_id: u32) -> Vec<RegisterationPlayer> {
        let mut registered = self
            .db_read_only()
            .tab_registeration_player()
            .registration_id()
            .filter(registration_id)
            .collect::<Vec<_>>();

        registered.sort_by_key(|f| f.registered_at);

        registered
    }
}
