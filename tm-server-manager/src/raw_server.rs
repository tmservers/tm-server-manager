use base64::Engine;
use base64::prelude::{BASE64_STANDARD, BASE64_URL_SAFE_NO_PAD};
use serde::Deserialize;
use spacetimedb::http::Request;
use spacetimedb::{
    DbContext, Identity, Local, ReducerContext, Table, Uuid, ViewContext, reducer, table,
};
use spacetimedb::{ProcedureContext, view};

use crate::authorization::Authorization;
use crate::competition::node::{NodeHandle, NodeRead};
use crate::competition::server_pool::TabCompetitionServerPoolRead;
use crate::raw_server::occupation::{TabRawServerOccupationRead, TabRawServerOccupationWrite};

pub mod config;
pub mod destination;
pub mod event;
pub mod method;
pub mod occupation;
pub mod player;
pub mod replay;

#[spacetimedb::table(accessor=tab_raw_server)]
pub struct RawServerV1 {
    #[unique]
    pub identity: Identity,
    #[unique]
    pub server_login: String,

    // Account id of the server from the trackmania web services.
    server_account_id: Uuid,

    /// Each server also has a ubisoft account associated with it.
    #[index(hash)]
    pub(crate) user_account_id: Uuid,

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
}

/// Elevates an annonymous user to a trackmania server.
/// password of the server doesn't get saved but rather verified for validity.
#[spacetimedb::procedure]
pub fn login_as_server(
    ctx: &mut ProcedureContext,
    login: String,
    password: String,
    user_account_id: Uuid,
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

    #[derive(Debug, Deserialize)]
    #[allow(non_snake_case)]
    struct NadeoServerToken {
        accessToken: String,
    }

    #[derive(Debug, Deserialize)]
    struct NadeoServerClaims {
        sub: String,
    }

    let mut body_string = result.into_body().into_string_lossy();

    let token =
        unsafe { json::from_str::<NadeoServerToken>(&mut body_string).map_err(|e| e.to_string())? };
    let payload = token.accessToken.split(".").collect::<Vec<_>>()[1].to_string();
    let mut payload = BASE64_URL_SAFE_NO_PAD.decode(payload).unwrap();
    let claims = json::from_slice::<NadeoServerClaims>(&mut payload).map_err(|e| e.to_string())?;

    let server_account_id = Uuid::parse_str(&claims.sub).unwrap();
    let identity = ctx.sender();

    ctx.try_with_tx::<(), String>(|ctx| {
        if let Some(mut server) = ctx.db.tab_raw_server().server_login().find(&login) {
            if server.server_account_id != user_account_id {
                server.verified = false;
            }
            server.set_online();
            server.set_identity(identity);
            ctx.db.tab_raw_server().id().update(server);
        } else {
            // Server has never been seen before so create a new one.
            ctx.db.tab_raw_server().try_insert(RawServerV1 {
                id: 0,
                server_login: login.clone(),
                server_account_id,
                user_account_id,
                identity,
                capturable: true,
                verified: false,
                online: true,
            })?;
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
        .user_account_id()
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
        .user_account_id()
        .filter(user.account_id)
        .filter(|s| s.verified && s.capturable)
        .filter(|s| !ctx.raw_server_is_occupied(s.id))
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
        .user_account_id()
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

    if server.user_account_id == user.account_id {
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

pub(crate) trait TabRawServerRead {}
pub(crate) trait TabRawServerWrite: TabRawServerRead {
    fn raw_server_pool_assign(&self, node_handle: NodeHandle) -> Result<u32, String>;
}

impl<Db: DbContext> TabRawServerRead for Db {}

impl<Db: DbContext<DbView = Local>> TabRawServerWrite for Db {
    fn raw_server_pool_assign(&self, node_handle: NodeHandle) -> Result<u32, String> {
        let available_servers = self.server_pool_available(self.node_get_parent(node_handle)?);
        if available_servers.is_empty() {
            return Err("No server is assigned to the match and there are no servers left to auto provision. Cannot start the match!".into());
        }

        let server_id = available_servers[0].id;

        self.raw_server_occupation_add(node_handle, server_id)?;

        Ok(server_id)
    }
}

//mod huh;
