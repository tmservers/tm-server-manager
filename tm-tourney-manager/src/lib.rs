use spacetimedb::ReducerContext;

use crate::server::tm_server;

pub mod auth;
pub mod competition;
pub mod emulator;
pub mod generator;
pub mod ghosts;
pub mod r#match;
pub mod record;
pub mod registration;
pub mod scheduling;
pub mod server;
pub mod tournament;
pub mod user;

pub mod graph;

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(client_connected))]
fn client_connected(ctx: &ReducerContext) -> Result<(), String> {
    // Execute if one tries to connect authenticated.
    if let Some(jwt) = ctx.sender_auth().jwt() {
        Ok(())
    } else {
        // The server comes back online.
        if let Some(mut server) = ctx.db.tm_server().identity().find(ctx.sender) {
            server.set_online();
            ctx.db.tm_server().id().update(server);
            Ok(())
        } else {
            // Client connects annonymously.
            Ok(())
        }
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(client_disconnected))]
fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(mut server) = ctx.db.tm_server().identity().find(ctx.sender) {
        server.set_offline();
        ctx.db.tm_server().id().update(server);
    }
}
