use spacetimedb::{ReducerContext, Table};

use crate::{
    server::tab_tm_server,
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
pub mod record;
pub mod registration;
pub mod scheduling;
pub mod server;
pub mod tournament;
pub mod user;
pub mod worker;

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(client_connected))]
fn client_connected(ctx: &ReducerContext) -> Result<(), String> {
    // Execute if one tries to connect authenticated.
    if let Some(jwt) = ctx.sender_auth().jwt() {
        log::warn!("Tried to connect with jwt");

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
        if let Some(mut server) = ctx.db.tab_tm_server().identity().find(ctx.sender) {
            server.set_online();
            ctx.db.tab_tm_server().tm_login().update(server);
            Ok(())
        } else {
            // Client connects annonymously.
            Ok(())
        }
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(client_disconnected))]
fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(mut server) = ctx.db.tab_tm_server().identity().find(ctx.sender) {
        server.set_offline();
        ctx.db.tab_tm_server().tm_login().update(server);
    }
}
