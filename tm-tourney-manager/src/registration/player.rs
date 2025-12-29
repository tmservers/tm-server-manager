use spacetimedb::{AnonymousViewContext, Query, ReducerContext, Table, Timestamp, reducer, table, view};

use crate::{auth::Authorization, competition::tab_competition};

#[table(name=tab_registered_player)]
pub struct RegisteredPlayer {
    #[index(btree)]
    competition_id: u32,
    account_id: String,
    registered_at: Timestamp,
}

#[view(name=registered_player,public)]
pub fn registered_player(ctx: &AnonymousViewContext) -> Query<RegisteredPlayer> {
    ctx.from
        .tab_registered_player()
        .build()
}

#[reducer]
pub fn register_player(ctx: &ReducerContext, competition_id: u32) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Tried to register for a competition that doesnt exist.".into());
    };

    //TODO validate in the compeition that registration is allowed.

    if ctx
        .db
        .tab_registered_player()
        .competition_id()
        .filter(competition_id)
        .any(|p| p.account_id == user)
    {
        return Err(format!(
            "User is already registered for competition {} ({competition_id})",
            competition.get_name()
        ));
    }

    ctx.db
        .tab_registered_player()
        .try_insert(RegisteredPlayer {
            competition_id,
            account_id: user,
            registered_at: ctx.timestamp,
        })?;

    Ok(())
}

#[reducer]
pub fn unregister_player(ctx: &ReducerContext, competition_id: u32) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Tried to register for a competition that doesnt exist.".into());
    };

    //TODO check if the registration period is over yet -> probably shouldnt be allowed to unregister if period is over.

    let Some(registred_user) = ctx
        .db
        .tab_registered_player()
        .competition_id()
        .filter(competition_id)
        .find(|p| p.account_id == user)
    else {
        return Err(format!(
            "User is already registered for competition {} ({competition_id})",
            competition.get_name()
        ));
    };

    if !ctx.db.tab_registered_player().delete(registred_user) {
        return Err(format!(
            "Unexpected error occured deleting the user {} from {}",
            user, competition_id
        ));
    };

    Ok(())
}
