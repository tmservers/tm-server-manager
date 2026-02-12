use spacetimedb::{ReducerContext, Table, Uuid, ViewContext, reducer, table, view};
use tm_server_types::config::ServerConfig;

use crate::{
    authorization::Authorization, r#match::tab_tm_match__view, raw_server::tab_raw_server__view,
};

#[table(name=tab_raw_server_config)]
pub struct TmRawServerConfig {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    // Creator of the Config
    account_id: Uuid,

    config: ServerConfig,
}

impl TmRawServerConfig {
    pub fn get_config(self) -> ServerConfig {
        self.config
    }
}

// The configuration that is owned by a server.
#[table(name=tab_raw_server_config_owned)]
pub struct TmRawServerConfigOwned {
    config: ServerConfig,

    #[primary_key]
    pub server_login: String,
}

impl TmRawServerConfigOwned {
    /// Returns a new defualt config
    pub(crate) fn new(server_login: String) -> Self {
        Self {
            config: ServerConfig::default(),
            server_login,
        }
    }
}

#[spacetimedb::reducer]
pub fn create_server_config(ctx: &ReducerContext, config: ServerConfig) -> Result<(), String> {
    let user = ctx.get_user()?;

    ctx.db
        .tab_raw_server_config()
        .try_insert(TmRawServerConfig {
            id: 0,
            account_id: user.account_id,
            config,
        })?;

    Ok(())
}

#[view(name=raw_server_config,public)]
fn raw_server_config(ctx: &ViewContext) -> Option<ServerConfig> {
    let server = ctx.db.tab_raw_server().identity().find(ctx.sender)?;

    /* let Some(tm_match) = ctx.db.tab_tm_match().id().find(server.active_match().unwrap()) else {
        return None;
    } */
    //TODO override if match is happening i guess.

    ctx.db
        .tab_raw_server_config_owned()
        .server_login()
        .find(&server.server_login)
        .map(|v| v.config)
}
