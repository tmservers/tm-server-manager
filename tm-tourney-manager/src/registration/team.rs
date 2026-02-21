use spacetimedb::{ReducerContext, Table, Timestamp, Uuid, reducer, table};

use crate::{authorization::Authorization, competition::tab_competition};

#[table(accessor=tab_registered_team)]
pub struct RegisteredTeam {
    account_id: Uuid,
    registered_at: Timestamp,
    #[index(btree)]
    competition_id: u32,
    name: String,
}

#[reducer]
pub fn create_team(ctx: &ReducerContext, competition_id: u32, name: String) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Tried to register for a competition that doesnt exist.".into());
    };

    //TODO validate in the compeition that registration is allowed.

    if ctx
        .db
        .tab_registered_team()
        .competition_id()
        .filter(competition_id)
        .any(|p| p.account_id == user.account_id)
    {
        return Err(format!(
            "User is already registered for competition {} ({competition_id})",
            competition.get_name()
        ));
    }

    ctx.db.tab_registered_team().try_insert(RegisteredTeam {
        competition_id,
        account_id: user.account_id,
        name,
        registered_at: ctx.timestamp,
    })?;

    Ok(())
}
