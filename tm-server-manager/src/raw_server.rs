use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use spacetimedb::http::Request;
use spacetimedb::{Identity, Query, ReducerContext, Table, Uuid, ViewContext, reducer, table};
use spacetimedb::{ProcedureContext, view};

use crate::authorization::Authorization;

pub mod config;
pub mod destination;
pub mod event;
pub mod method;
pub mod player;
pub mod replay;

#[spacetimedb::table(accessor=tab_raw_server)]
pub struct RawServerV1 {
    #[unique]
    pub identity: Identity,
    /// Trackmania server logins are unique.
    #[unique]
    pub server_login: String,

    /// Each server also has a ubisoft account associated with it.
    #[index(hash)]
    pub(crate) account_id: Uuid,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

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
    /* pub fn set_config(&mut self, config: ServerConfig) {
        self.config = config
    } */

    /* pub fn set_state(&mut self, state: ServerState) {
        self.state = state
    } */

    pub fn set_online(&mut self) {
        self.online = true;
    }
    pub fn set_offline(&mut self) {
        self.online = false;
    }

    pub fn set_identity(&mut self, identity: Identity) {
        self.identity = identity;
    }

    pub fn is_verified(&self) -> bool {
        self.verified
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

#[table(accessor=tab_raw_server_occupation)]
pub struct RawServerOccupation {
    #[primary_key]
    pub(crate) server_id: u32,

    #[index(hash)]
    match_id: u32,
}

impl RawServerOccupation {
    pub(crate) fn new(match_id: u32, server_id: u32) -> Self {
        Self {
            server_id,
            match_id,
        }
    }
}

/// Elevates an annonymous user to a trackmania server.
/// password of the server doesn't get saved but rather verified for validity.
#[spacetimedb::procedure]
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
        .header("User-Agent", "tm-server-manager | central")
        .body(r#"{ "audience": "NadeoServices" }"#)
        //TODO see what would be a good error message
        .map_err(|e| e.to_string())?;
    let result = ctx
        .http
        .send(request)
        .map_err(|e| format!("Internal Error! The HTTP request could not be sent! Error: {e}"))?;

    let status = result.status();

    if !status.is_success() {
        log::error!("Login attempt from server ({}) was not a success", login);
        return Err("Server registration failed because credential were wrong".into());
    }

    let identity = ctx.sender();

    ctx.try_with_tx::<(), String>(|ctx| {
        if let Some(mut server) = ctx.db.tab_raw_server().server_login().find(&login) {
            // The new identity is assigned to the server.
            server.set_identity(identity);
            server.set_online();
            ctx.db.tab_raw_server().id().update(server);
        } else {
            // Server has never been seen before so create a new one.
            let server = ctx.db.tab_raw_server().try_insert(RawServerV1 {
                id: 0,
                server_login: login.clone(),
                account_id,
                identity,
                capturable: true,
                verified: false,
                online: true,
            })?;
            //TODO which config should be active?
            /* ctx.db
            .tab_raw_server_config_active()
            .try_insert(RawServerConfigActive::new(server.server_login))?; */
        }
        Ok(())
    })?;

    Ok(())
}

#[view(accessor= this_raw_server, public)]
fn this_raw_server(ctx: &ViewContext) -> Option<RawServerV1> {
    ctx.db.tab_raw_server().identity().find(ctx.sender())
}

/// The Raw server pool are all servers of an account which are verified.
#[view(accessor= user_raw_server_pool, public)]
pub(crate) fn user_raw_server_pool(ctx: &ViewContext) -> Vec<RawServerV1> {
    let Ok(user) = ctx.get_user() else {
        return Vec::new();
    };
    //TODO maybe switch to query builder if possible
    ctx.db
        .tab_raw_server()
        .account_id()
        .filter(user.account_id)
        .filter(|s| s.verified)
        .collect()
}

/// The Raw server pool are all servers of an account which are verified.
#[view(accessor= user_available_server_pool, public)]
pub(crate) fn user_available_server_pool(ctx: &ViewContext) -> Vec<RawServerV1> {
    let Ok(user) = ctx.get_user() else {
        return Vec::new();
    };

    ctx.db
        .tab_raw_server()
        .account_id()
        .filter(user.account_id)
        .filter(|s| s.verified)
        .filter(|s| {
            ctx.db
                .tab_raw_server_occupation()
                .server_id()
                .find(s.id)
                .is_none()
        })
        .collect()
}

/// The unverified version of a server pool includes all servers of an account which are not vet verified.
#[view(accessor= user_raw_server_pool_unverified, public)]
fn user_raw_server_pool_unverified(ctx: &ViewContext) -> Vec<RawServerV1> {
    let Ok(user) = ctx.get_user() else {
        return Vec::new();
    };
    //TODO maybe switch to query builder if possible
    ctx.db
        .tab_raw_server()
        .account_id()
        .filter(user.account_id)
        .filter(|s| !s.verified)
        .collect()
}

#[reducer]
fn raw_server_verify(ctx: &ReducerContext, server_id: u32) -> Result<(), String> {
    let user = ctx.get_user()?;

    let mut server = ctx
        .db
        .tab_raw_server()
        .id()
        .find(server_id)
        .ok_or("Couldnt find server with login")?;

    if server.account_id == user.account_id {
        if server.verified {
            Err("Server was already verified.".into())
        } else {
            server.verified = true;
            ctx.db.tab_raw_server().id().update(server);
            Ok(())
        }
    } else {
        Err("Not permitted to edit the server".into())
    }
}
