use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use spacetimedb::http::Request;
use spacetimedb::{Identity, Query, ReducerContext, Table, Uuid, ViewContext};
use spacetimedb::{ProcedureContext, view};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::raw_server::config::{
    TmRawServerConfig, TmRawServerConfigOwned, tab_raw_server_config, tab_raw_server_config_owned,
};

pub mod config;
pub mod event;
pub mod method;
pub mod state;

#[spacetimedb::table(name=tab_raw_server,public)]
pub struct RawServerV1 {
    #[unique]
    pub identity: Identity,
    /// Each server also has a ubisoft account associated with it.
    #[index(btree)]
    account_id: Uuid,

    /// Trackmania server logins are unique.
    #[primary_key]
    pub server_login: String,

    active_match: Option<u32>,
    // Whether the server can be reached with a bridge active.
    online: bool,

    // Can the server be provisioned or is it a fixed server?
    capturable: bool,

    // This is necessary because at the moment a arbitrary account_id can be supplied when logging in as a server
    // as there is no way to verify it through the trackmania web services.
    // To avoid adding servers to a the pool of a user without verification (which could be an attack vector) we require manual verification from the user.
    verified: bool,
}

impl RawServerV1 {
    pub fn active_match(&self) -> Option<u32> {
        self.active_match
    }

    pub fn set_active_match(&mut self, match_id: u32) {
        if self.active_match.is_none() {
            self.active_match = Some(match_id)
        }
    }

    /* pub fn set_config(&mut self, config: ServerConfig) {
        self.config = config
    } */

    /* pub fn set_state(&mut self, state: ServerState) {
        self.state = state
    } */

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

    /* pub(crate) fn add_server_event(&mut self, event: &Event) -> bool {
        match event {
            Event::PlayerConenct(player) => log::error!("Player connected: {}", player.account_id),
            _ => return false,
        }
        log::warn!("{:#?}", self.state);
        true
    } */

    /* pub fn set_command(&mut self, command: Method) {
    self.server_method = command
    } */
}

/// Elevates an annonymous user to a trackmania server.
/// password of the server doesn't get saved but rather verified for validity.
#[cfg_attr(feature = "spacetime", spacetimedb::procedure)]
pub fn login_as_server(
    ctx: &mut ProcedureContext,
    login: String,
    password: String,
    account_id: Uuid,
) -> Result<(), String> {
    let request = Request::builder()
        .method("POST")
        .uri("https://prod.trackmania.core.nadeo.online/v2/authentication/token/basic")
        .header(
            "Authorization",
            format!(
                "Basic {}",
                BASE64_STANDARD.encode(login.clone() + ":" + &password)
            ),
        )
        .header("Content-Type", "application/json")
        .header("User-Agent", "tm-tourney-manager | central")
        .body(r#"{ "audience": "NadeoServices" }"#)
        //TODO see what would be a good error message
        .map_err(|e| e.to_string())?;
    let result = ctx.http.send(request).unwrap();

    let status = result.status();

    if !status.is_success() {
        log::error!("API request was not a success");
        return Err("Server registration failed because credential were wrong".into());
    }

    let identity = ctx.sender;

    ctx.try_with_tx::<(), String>(|ctx| {
        /* if ctx
            .db
            .tab_raw_server_online()
            .identity()
            .find(ctx.sender)
            .is_some()
        {
            // Server identity is already verified.
            // return Ok(());
        } */
        if let Some(mut server) = ctx.db.tab_raw_server().server_login().find(&login) {
            // The new identity is assigned to the server.
            server.set_identity(ctx.identity());
            ctx.db.tab_raw_server().server_login().update(server);
        } else {
            // Server has never been seen before so create a new one.

            let server = ctx.db.tab_raw_server().try_insert(RawServerV1 {
                //online: true,
                server_login: login.clone(),
                active_match: None,
                account_id,
                identity,
                capturable: true,
                verified: false,
                online: true,
            })?;
            ctx.db
                .tab_raw_server_config_owned()
                .try_insert(TmRawServerConfigOwned::new(server.server_login))?;
        }
        Ok(())
    })?;

    Ok(())
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

#[view(name = this_raw_server, public)]
fn this_raw_server(ctx: &ViewContext) -> Option<RawServerV1> {
    ctx.db.tab_raw_server().identity().find(ctx.sender)
}

#[view(name = raw_server, public)]
fn raw_server(ctx: &ViewContext) -> Query<RawServerV1> {
    //ctx.db.tab_tm_server().identity().find(ctx.sender)
    //TODO access control.
    // User should see his servers.
    // Server should see himself
    // Worker should see nothing
    ctx.from.tab_raw_server().build()
}

#[view(name = raw_server_expected_players, public)]
fn raw_server_expected_players(ctx: &ViewContext) -> Vec</* PlayerEntry */ RawServerV1> {
    //TODO make player entry struct
    let Some(server) = ctx.db.tab_raw_server().identity().find(ctx.sender) else {
        return Vec::new();
    };

    if let Some(match_id) = server.active_match() {
        //TODO convert the match_id to the list with the connection filter
        Vec::new()
    } else {
        Vec::new()
    }
}

#[view(name = raw_server_current_players, public)]
fn raw_server_current_players(ctx: &ViewContext) -> Vec</* PlayerEntry */ RawServerV1> {
    //TODO make player entry struct
    let Some(server) = ctx.db.tab_raw_server().identity().find(ctx.sender) else {
        return Vec::new();
    };

    Vec::new()
}
