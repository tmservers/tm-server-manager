use spacetimedb::{ReducerContext, Table};

use crate::{
    raw_server::{tab_raw_server_offline, tab_raw_server_online},
    user::{User as UserStruct, UserIdentity, tab_user as db_user, user_identity},
};

pub mod auth;
pub mod competition;
pub mod emulator;
pub mod environment;
pub mod generator;
pub mod ghosts;
pub mod r#match;
pub mod monitoring;
pub mod raw_server;
pub mod record;
pub mod registration;
pub mod scheduling;
pub mod tournament;
pub mod user;
pub mod worker;

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(client_connected))]
fn client_connected(ctx: &ReducerContext) -> Result<(), String> {
    // Execute if one tries to connect authenticated.
    if let Some(jwt) = ctx.sender_auth().jwt() {
        log::warn!("Tried to connect with jwt {}", jwt.raw_payload());

        //TODO get trackmania id claim.
        let account_id = String::from("3467014a-c1cc-4aae-99fe-6beb5eca232a");
        let preferred_username = String::from("Mr.Joermungandr");

        ctx.db
            .tab_user()
            .try_insert(UserStruct::new(account_id.clone(), preferred_username))?;
        ctx.db
            .user_identity()
            .try_insert(UserIdentity::new(account_id, ctx.identity()))?;

        Ok(())
    } else {
        // The server comes back online.
        if let Some(mut server) = ctx.db.tab_raw_server_offline().identity().find(ctx.sender) {
            // server.set_online();
            ctx.db.tab_raw_server_online().tm_login().update(server);
            Ok(())
        } else {
            // Client connects annonymously.
            Ok(())
        }
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(client_disconnected))]
fn client_disconnected(ctx: &ReducerContext) {
    if let Some(mut server) = ctx.db.tab_raw_server_online().identity().find(ctx.sender) {
        // server.set_offline();
        ctx.db.tab_raw_server_online().tm_login().update(server);
    }
}
