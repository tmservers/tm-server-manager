use spacetimedb::{SpacetimeType, ViewContext, table, view};
use tm_server_types::config::ServerConfig;

use crate::{
    competition::node::NodeRead,
    raw_server::{
        method::TmServerMethodResponse, occupation::TabRawServerOccupationRead,
        tab_raw_server__view,
    },
    tm_match::{MatchStatus, tab_match__view},
    tm_server::tab_server__view,
};

#[table(accessor=tab_raw_server_config)]
pub struct RawServerConfig {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    config: ServerConfig,
}

impl RawServerConfig {
    /* pub fn get_config(self) -> ServerConfig {
        self.config
    } */

    pub fn new(config: ServerConfig) -> Self {
        Self { id: 0, config }
    }
}

// The configuration that is owned by a server.
/* #[table(accessor=tab_raw_server_config_active)]
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

/* #[spacetimedb::reducer]
pub fn create_server_config(ctx: &ReducerContext, config: ServerConfig) -> Result<(), String> {
    let user = ctx.get_user()?;

    ctx.db.tab_raw_server_config().try_insert(RawServerConfig {
        id: 0,
        //account_id: user.account_id,
        config,
    })?;

    Ok(())
} */

#[derive(Debug, SpacetimeType)]
struct ServerMetadata {
    config: ServerConfig,
    open: bool,
    //status: MatchStatus,
}

#[view(accessor=raw_server_config,public)]
fn raw_server_config(ctx: &ViewContext) -> Option<ServerMetadata> {
    let server = ctx.db.tab_raw_server().identity().find(ctx.sender())?;

    let node = ctx.raw_server_occupation(server.id)?;

    match node {
        crate::competition::node::NodeHandle::MatchV1(m) => {
            let tm_match = ctx.db.tab_match().id().find(m).unwrap();
            let config = ctx
                .db
                .tab_raw_server_config()
                .id()
                .find(tm_match.get_config_id())?;
            Some(ServerMetadata {
                config: config.config,
                open: tm_match.is_open(),
            })
        }
        crate::competition::node::NodeHandle::ServerV1(s) => {
            let tm_server = ctx.db.tab_server().id().find(s).unwrap();
            let config = ctx
                .db
                .tab_raw_server_config()
                .id()
                .find(tm_server.get_config_id())?;
            Some(ServerMetadata {
                config: config.config,
                open: tm_server.is_open(),
            })
        }
        _ => {
            log::error!("Requested a configuration from a node type other than Match or Server?");
            None
        }
    }
}
