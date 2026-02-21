use spacetimedb::{ReducerContext, Table, reducer, table};

#[table(accessor=env,private)]
pub(crate) struct Env {
    #[primary_key]
    pub(crate) key: String,
    pub(crate) value: String,
}

//Allows module owners to
#[reducer]
fn create_env_var(ctx: &ReducerContext, key: String, value: String) -> Result<(), String> {
    //TODO only modult owners should be able to do this.

    ctx.db.env().try_insert(Env { key, value })?;

    Ok(())
}
