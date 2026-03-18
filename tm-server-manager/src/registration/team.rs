use spacetimedb::{ReducerContext, Table, Timestamp, Uuid, reducer, table};

use crate::{
    authorization::Authorization, competition::tab_competition, registration::tab_registration,
};

#[table(accessor=tab_registered_team)]
pub struct RegisteredTeam {
    name: String,
    //account_id: Uuid,
    registered_at: Timestamp,
    #[index(btree)]
    registration_id: u32,

    #[auto_inc]
    #[primary_key]
    id: u32,
}

#[reducer]
pub fn create_team(ctx: &ReducerContext, registration_id: u32, name: String) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Tried to register for a registration that doesnt exist.".into());
    };

    if registration.team_registration_allowed(ctx) {
        return Err("Team registration not open.".into());
    }

    /* if ctx
        .db
        .tab_registered_team()
        .registration_id()
        .filter(registration_id)
    //.any(|p| p.account_id == user.account_id)
    {
        return Err(format!(
            "User is already registered for registration {} ({registration_id})",
            registration.name
        ));
    } */

    ctx.db.tab_registered_team().try_insert(RegisteredTeam {
        registration_id,
        //account_id: user.account_id,
        name,
        registered_at: ctx.timestamp,
        id: 0,
    })?;

    Ok(())
}
