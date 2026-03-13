use spacetimedb::{ReducerContext, Table, Uuid, reducer, table};
use tm_server_types::event::Event;

use crate::{
    authorization::Authorization,
    raw_server::{
        player::{raw_server_player_add, raw_server_player_remove, tab_raw_server_player},
        tab_raw_server, tab_raw_server_occupation,
    },
    tm_match::{
        event::handle_match_event,
        state::{MatchState, tab_match_state},
        tab_match,
    },
};

/// Servers call this to post the event stream.
#[reducer]
pub fn post_event(ctx: &ReducerContext, event: Event) -> Result<(), String> {
    let server = ctx.get_server()?;

    match &event {
        Event::PlayerConenct(player) => raw_server_player_add(
            ctx,
            Uuid::parse_str(&player.account_id).unwrap(),
            player.is_spectator,
        )?,
        Event::PlayerDisconnect(player) => {
            raw_server_player_remove(ctx, Uuid::parse_str(&player.account_id).unwrap())?
        }
        Event::PlayerInfoChanged(player) => {
            let spectator = player.spectator_status != 0;
            raw_server_player_add(ctx, Uuid::parse_str(&player.account_id).unwrap(), spectator)?
        }
        _ => (),
    }

    if let Some(occupation) = ctx
        .db
        .tab_raw_server_occupation()
        .server_id()
        .find(server.id)
        && let Some(tm_match) = ctx.db.tab_match().id().find(occupation.match_id)
        && tm_match.is_live()
    {
        handle_match_event(ctx, tm_match.id, event)?
    }
    Ok(())
}
