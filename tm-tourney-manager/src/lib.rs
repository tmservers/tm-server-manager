use spacetimedb::{ReducerContext, reducer};

use crate::server::tm_server;

pub mod auth;
pub mod competition;
pub mod emulator;
pub mod generator;
pub mod ghosts;
pub mod r#match;
pub mod registration;
pub mod scheduling;
pub mod server;
pub mod tournament;
pub mod user;

pub mod graph;

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(init))]
fn init(_ctx: &ReducerContext) {
    /* let _ten_seconds = TimeDuration::from_micros(10_000_000);
    /* ctx.db.send_message_schedule().insert(SendMessageSchedule {
        scheduled_id: 0,
        text: "I'm a bot sending a message every 10 seconds".to_string(),

        // Creating a `ScheduleAt` from a `Duration` results in the reducer
        // being called in a loop, once every `loop_duration`.
        scheduled_at: ten_seconds.into(),
    }); */ */
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer(client_connected))]
// Called when a client connects to a SpacetimeDB database server
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
// Called when a client disconnects from SpacetimeDB database server
fn identity_disconnected(ctx: &ReducerContext) {
    if let Some(mut server) = ctx.db.tm_server().identity().find(ctx.sender) {
        server.set_offline();
        ctx.db.tm_server().id().update(server);
    }
}
