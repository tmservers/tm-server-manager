use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use spacetimedb::http::Request;
use spacetimedb::{Identity, Query, ReducerContext, Table, ViewContext};
use spacetimedb::{ProcedureContext, view};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::server::{config::tm_server_config, state::ServerState};

pub mod config;
pub mod event;
pub mod method;
pub mod state;

//TODO maybe rename to RawServerV1
#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tab_tm_server))]
pub struct TmServerV1 {
    /// Trackmania server logins are unique.
    #[primary_key]
    pub tm_login: String,
    #[unique]
    pub identity: Identity,

    /// Each server also has a ubisoft account associated with it.
    #[index(btree)]
    owner_id: String,

    // Whether the server can be reached with a bridge active.
    online: bool,

    config: ServerConfig,

    // Mutable state which the server reacts to.
    state: ServerState,

    // Can the server be provisioned or is it a fixed server?
    capturable: bool,

    // This is necessary because at the moment a arbitrary account_id can be supplied when logging in as a server
    // as there is no way to verify it through the trackmania web services.
    // To avoid adding servers to a the pool of a user without verification (which could be an attack vector) we require manual verification from the user.
    verified: bool,

    active_match: Option<u32>,
}

impl TmServerV1 {
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
            Event::PlayerConenct(player) => log::error!("Player connected: {}", player.account_id),
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
#[cfg_attr(feature = "spacetime", spacetimedb::procedure)]
pub fn login_as_server(
    ctx: &mut ProcedureContext,
    login: String,
    password: String,
    account_id: String,
)
/* -> Result<(), String> */
{
    /*  let request = Request::builder()
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
                .unwrap();
            let result = ctx.http.send(request).unwrap();

            let status = result.status();

            if status.is_success() {
                let body = result.into_body();
                let string = body.into_string().unwrap();
                //let string = BASE64_STANDARD.decode(string);
                log::error!("{:?}", string)
    } else {
        //TODO error handling
        log::error!("Server registration failed because of nadeo request");
        panic!()
        } */

    ctx.with_tx(|ctx| {
        if ctx.db.tab_tm_server().identity().find(ctx.sender).is_some() {
            // Server identity is already verified.
            // return Ok(());
        }
        if let Some(mut server) = ctx.db.tab_tm_server().tm_login().find(&login) {
            // The new identity is assigned to the server.
            server.set_identity(ctx.identity());
            ctx.db.tab_tm_server().tm_login().update(server);
            //Ok(())
        } else {
            //TODO make HTTP call when its available and verify that credentials are correct.

            // Server has never been seen before so create a new one.
            ctx.db.tab_tm_server().insert(TmServerV1 {
                online: true,
                tm_login: login.clone(),
                active_match: None,
                //TODO obtain userid from HTTP request
                owner_id: account_id.clone(),
                // server_method: None,
                config: ServerConfig::default(),
                state: ServerState::default(),
                identity: ctx.identity(),
                capturable: true,
                verified: false,
            });
            //Ok(())
        }
    });
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

#[view(name = this_tm_server, public)]
fn this_tm_server(ctx: &ViewContext) -> Option<TmServerV1> {
    ctx.db.tab_tm_server().identity().find(ctx.sender)
}

#[view(name = tm_server, public)]
fn tm_server(ctx: &ViewContext) -> Query<TmServerV1> {
    //ctx.db.tab_tm_server().identity().find(ctx.sender)
    //TODO access control.
    // User should see his servers.
    // Server should see himself
    // Worker should see nothing
    ctx.from.tab_tm_server().build()
}
