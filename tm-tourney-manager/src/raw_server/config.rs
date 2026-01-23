use spacetimedb::{ReducerContext, Table, Uuid, reducer, table};
use tm_server_types::config::ServerConfig;

use crate::authorization::Authorization;

#[table(name=tm_server_config, public)]
pub struct TmServerConfig {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    // Creator of the Config
    account_id: Uuid,

    config: ServerConfig,
}

impl TmServerConfig {
    pub fn get_config(self) -> ServerConfig {
        self.config
    }
}

#[spacetimedb::reducer]
pub fn create_server_config(ctx: &ReducerContext, config: ServerConfig) -> Result<(), String> {
    let user = ctx.get_user()?;

    ctx.db.tm_server_config().try_insert(TmServerConfig {
        id: 0,
        account_id: user.account_id,
        config,
    })?;

    Ok(())
}
