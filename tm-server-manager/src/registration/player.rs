use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, Table, Timestamp, Uuid, reducer, table, view,
};

use crate::{
    authorization::Authorization, competition::tab_competition, registration::tab_registration,
};

#[table(accessor=tab_registered_player)]
pub struct RegisteredPlayer {
    account_id: Uuid,
    registered_at: Timestamp,
    #[index(hash)]
    registration_id: u32,
}

#[view(accessor=registered_player,public)]
pub fn registered_player(
    ctx: &AnonymousViewContext, /* ,registration_id: u32 */
) -> impl Query<RegisteredPlayer> {
    let registration_id = 1u32;
    ctx.from
        .tab_registered_player()
        .r#where(|c| c.registration_id.eq(registration_id))
}

#[reducer]
pub fn register_player(ctx: &ReducerContext, registration_id: u32) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Tried to register but the registration id does not exist.".into());
    };

    //TODO validate that registration is allowed.

    if ctx
        .db
        .tab_registered_player()
        .registration_id()
        .filter(registration_id)
        .any(|p| p.account_id == user.account_id)
    {
        return Err("User is already registered for registration_id!".to_string());
    }

    ctx.db
        .tab_registered_player()
        .try_insert(RegisteredPlayer {
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

    //TODO check if the registration period is over yet -> probably shouldnt be allowed to unregister if period is over.

    let Some(registred_user) = ctx
        .db
        .tab_registered_player()
        .registration_id()
        .filter(registration_id)
        .find(|p| p.account_id == account_id)
    else {
        return Err("User is already registered for competition!".to_string());
    };

    if !ctx.db.tab_registered_player().delete(registred_user) {
        return Err(format!(
            "Unexpected error occured deleting the user {} from {}",
            account_id, registration_id
        ));
    };

    Ok(())
}
