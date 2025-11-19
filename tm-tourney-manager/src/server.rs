use spacetimedb::view;
use spacetimedb::{Identity, ReducerContext, Table, ViewContext};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::server::{config::tm_server_config, state::ServerState};

pub mod config;
pub mod event;
pub mod method;
pub mod state;

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tm_server, public))]
pub struct TmServer {
    /// Trackmania server logins are unique.
    #[primary_key]
    pub id: String,
    #[unique]
    pub identity: Identity,

    /// Each server also has a ubisoft account associated with it.
    owner_id: String,

    // Whether the server can be reached and has a bridge active.
    online: bool,

    config: ServerConfig,

    // Mutable state which the server reacts to.
    state: ServerState,

    // Can the server be provisioned or is it a fixed server?
    capturable: bool,

    active_match: Option<u32>,
}

#[view(name = this_tm_server, public)]
fn this_tm_server(ctx: &ViewContext) -> Option<TmServer> {
    ctx.db.tm_server().identity().find(ctx.sender)
}

impl TmServer {
    pub fn active_match(&self) -> Option<u32> {
        self.active_match
    }

    pub fn set_active_match(&mut self, match_id: u32) {
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

    pub(crate) fn release(&mut self) {
        self.active_match = None;
    }

    pub fn set_online(&mut self) {
        self.online = true;
    }
    pub fn set_offline(&mut self) {
        self.online = false;
    }

    pub fn set_identity(&mut self, identity: Identity) {
        self.identity = identity;
    }

    pub(crate) fn add_server_event(&mut self, event: &Event) -> bool {
        match event {
            Event::PlayerConenct(player) => log::error!("Player connected: {}", player.login),
            _ => return false,
        }
        log::warn!("{:#?}", self.state);
        true
    }

    /* pub fn set_command(&mut self, command: Method) {
        self.server_method = command
    } */
}

/// Elevates an annonymous user to a trackmania server.
/// password of the server doesn't get saved but rather verified for validity.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn register_server(
    ctx: &ReducerContext,
    login: String,
    password: String,
) -> Result<(), String> {
    if ctx.db.tm_server().identity().find(ctx.sender).is_some() {
        // Server identity is already verified.
        return Ok(());
    }
    if let Some(mut server) = ctx.db.tm_server().id().find(&login) {
        // The new identity is assigned to the server.
        server.set_identity(ctx.identity());
        ctx.db.tm_server().id().update(server);
        Ok(())
    } else {
        //TODO make HTTP call when its available and verify that credentials are correct.

        // Server has never been seen before so create a new one.
        ctx.db.tm_server().insert(TmServer {
            online: true,
            id: login,
            active_match: None,
            //TODO obtain userid from HTTP request
            owner_id: "test_user".into(),
            // server_method: None,
            config: ServerConfig::default(),
            state: ServerState::default(),
            identity: ctx.identity(),
            capturable: true,
        });
        Ok(())
    }
}

/* #[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn load_server_config(ctx: &ReducerContext, id: String, with_config: u32) {
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
 */
