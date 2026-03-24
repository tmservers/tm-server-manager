use spacetimedb::{ReducerContext, Table, Uuid, reducer, table};
use tm_server_types::event::Event;

use crate::{
    authorization::Authorization,
    raw_server::{
        occupation::TabRawServerOccupationRead,
        player::{raw_server_player_add, raw_server_player_remove, tab_raw_server_player},
        tab_raw_server,
    },
    tm_match::{
        event::handle_match_event,
        state::{MatchState, tab_match_state},
        tab_match,
    },
    user::{UserIdsMap, UserV1, tab_user, tab_user_ids_map},
};

/// Servers call this to post the event stream.
#[reducer]
pub fn post_event(ctx: &ReducerContext, event: Event) -> Result<(), String> {
    let server = ctx.get_server()?;

    match &event {
        Event::PlayerConnect(player) => {
            let account_id = Uuid::parse_str(&player.account_id).unwrap();
            if ctx.db.tab_user().account_id().find(account_id).is_none() {
                let user = ctx.db.tab_user().try_insert(UserV1::new(account_id))?;
                ctx.db
                    .tab_user_ids_map()
                    .try_insert(UserIdsMap::new(account_id, user.internal_id))?;
            }
            raw_server_player_add(ctx, account_id, player.is_spectator)?
        }
        Event::PlayerDisconnect(player) => {
            raw_server_player_remove(ctx, Uuid::parse_str(&player.account_id).unwrap())?
        }
        Event::PlayerInfoChanged(player) => {
            let spectator = player.spectator_status != 0;
            raw_server_player_add(ctx, Uuid::parse_str(&player.account_id).unwrap(), spectator)?
        }
        _ => (),
    }

    if let Some(node) = ctx.raw_server_occupation(server.id) {
        if node.is_match()
            && let Some(tm_match) = ctx.db.tab_match().id().find(node.split().1)
            && tm_match.is_live()
        {
            handle_match_event(ctx, tm_match.id, event)?
        }

        if node.is_server() {
            //TODO handle server events.
        }
    }
    Ok(())
}
