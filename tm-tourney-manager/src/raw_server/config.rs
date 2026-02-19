use spacetimedb::{ReducerContext, Table, Uuid, ViewContext, reducer, table, view};
use tm_server_types::config::ServerConfig;

use crate::{
    authorization::Authorization,
    r#match::tab_tm_match__view,
    raw_server::{tab_raw_server__view, tab_raw_server_occupation__view},
};

#[table(name=tab_raw_server_config)]
pub struct RawServerConfig {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    //TODO probably remove that and add it as a component table.
    // Creator of the Config
    account_id: Uuid,

    config: ServerConfig,
}

impl RawServerConfig {
    pub fn get_config(self) -> ServerConfig {
        self.config
    }
}

// The configuration that is owned by a server.
/* #[table(name=tab_raw_server_config_active)]
pub struct RawServerConfigActive {
    config: u32,

    #[primary_key]
    pub server_login: String,
}

impl RawServerConfigActive {
    /// Returns a new defualt config
    pub(crate) fn new(server_login: String) -> Self {
        Self {
            //TODO
            config: 0,
            server_login,
        }
    }
} */

#[spacetimedb::reducer]
pub fn create_server_config(ctx: &ReducerContext, config: ServerConfig) -> Result<(), String> {
    let user = ctx.get_user()?;

    ctx.db.tab_raw_server_config().try_insert(RawServerConfig {
        id: 0,
        account_id: user.account_id,
        config,
    })?;

    Ok(())
}

#[view(name=raw_server_config,public)]
fn raw_server_config(ctx: &ViewContext) -> Option<ServerConfig> {
    let server = ctx.db.tab_raw_server().identity().find(ctx.sender)?;

    let server_occupation = ctx
        .db
        .tab_raw_server_occupation()
        .server_id()
        .find(server.id)?;

    let tm_match = ctx
        .db
        .tab_tm_match()
        .id()
        .find(server_occupation.match_id)?;

    let config = ctx
        .db
        .tab_raw_server_config()
        .id()
        .find(tm_match.get_config_id())?;

    Some(config.config)
}
