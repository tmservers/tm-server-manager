use spacetimedb::{ReducerContext, Table, reducer, table};

#[table(accessor=env,private)]
pub(crate) struct Env {
    #[primary_key]
    pub(crate) key: String,
    pub(crate) value: String,
}

#[reducer]
fn set_env_var(ctx: &ReducerContext, key: String, value: String) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Not permitted to set environment variable.".into());
    }

    ctx.db.env().try_insert(Env { key, value })?;

    Ok(())
}
