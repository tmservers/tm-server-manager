use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};
use tm_server_types::{config::ServerConfig, method::Method};

use crate::server::{
    config::{TmServerConfig, tm_server_config},
    state::ServerState,
};

pub mod config;
pub mod event;
pub mod state;

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tm_server, public))]
pub struct TmServer {
    /// Trackmania provisiones a unique server_id for each server.
    //#[unique]
    #[primary_key]
    pub id: String,

    /// Each server also has a ubisoft account associated with it.
    owner_id: String,

    online: bool,

    config: ServerConfig,

    state: ServerState,

    active_match: Option<u64>,

    // TODO: Properly enfoce the protocol.
    /// On every update call this MUST be set to None EXCEPT you want to call a method.
    server_method: Option<Method>,
}

/* #[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum ServerState {
    Available,
    Provisioned,
} */

impl TmServer {
    pub fn active_match(&self) -> Option<u64> {
        self.active_match
    }

    pub fn set_active_match(&mut self, match_id: u64) {
        if self.active_match.is_none() {
            self.active_match = Some(match_id)
        }
    }

    pub fn set_config(&mut self, config: ServerConfig) {
        self.config = config
    }

    pub fn set_state(&mut self, state: ServerState) {
        self.state = state
    }

    /* pub fn set_command(&mut self, command: Method) {
        self.server_method = command
    } */
}

#[cfg(feature = "development")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn add_server(ctx: &ReducerContext, id: String) {
    ctx.db.tm_server().insert(TmServer {
        online: true,
        id,
        active_match: None,
        owner_id: "test_user".into(),
        server_method: None,
        config: ServerConfig::default(),
        state: ServerState::default(),
    });
}

#[cfg(feature = "development")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn call_server(ctx: &ReducerContext, id: String, method: Method) {
    if let Some(server) = ctx.db.tm_server().id().find(id) {
        ctx.db.tm_server().id().update(TmServer {
            server_method: Some(method),
            ..server
        });
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn load_server_config(ctx: &ReducerContext, id: String, with_config: u64) {
    if let Some(mut server) = ctx.db.tm_server().id().find(id)
        && let Some(config) = ctx.db.tm_server_config().id().find(with_config)
    {
        server.set_config(config.get_config());
        ctx.db.tm_server().id().update(server);
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn set_tm_server_state(ctx: &ReducerContext, id: String, state: ServerState) {
    if let Some(mut server) = ctx.db.tm_server().id().find(id) {
        server.set_state(state);
        ctx.db.tm_server().id().update(server);
    }
}
