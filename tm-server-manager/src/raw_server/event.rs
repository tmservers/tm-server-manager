use spacetimedb::{ReducerContext, Uuid, reducer};
use tm_server_types::event::Event;

use crate::{
    authorization::Authorization,
    raw_server::{
        occupation::TabRawServerOccupationRead,
        player::{raw_server_player_add, raw_server_player_remove},
    },
    tm_match::{
        chat::{MatchChat, MatchChatWrite},
        event::handle_match_event,
        state::MatchStateRead,
    },
    user::UserWrite,
};

/// Servers call this to post the event stream.
#[reducer]
pub(crate) fn post_event(ctx: &ReducerContext, event: Event) -> Result<(), String> {
    let server_id = ctx.server_id()?;

    match &event {
        Event::PlayerConnect(player) => {
            log::info!("Player connected: {}", player.account_id);
            let account_id = Uuid::parse_str(&player.account_id).unwrap();
            if let Err(err) = ctx.user_id_or_insert(account_id) {
                log::error!("{err}");
            }
            if let Err(err) = raw_server_player_add(ctx, account_id, player.is_spectator) {
                log::error!("Player Disconnect: {err}");
            };
        }
        Event::PlayerDisconnect(player) => {
            if let Err(err) =
                raw_server_player_remove(ctx, Uuid::parse_str(&player.account_id).unwrap())
            {
                log::error!("Player Disconnect: {err}");
            };
        }
        Event::PlayerInfoChanged(player) => {
            let spectator = player.spectator_status != 0;
            let account_id = Uuid::parse_str(&player.account_id).unwrap();
            ctx.user_update_name(account_id, player.nick_name.clone());
            if let Err(err) = raw_server_player_add(ctx, account_id, spectator) {
                log::error!("Player Disconnect: {err}");
            };
        }
        _ => (),
    }

    if let Some(node) = ctx.raw_server_occupation(server_id) {
        if node.is_match()
            && let Some(state) = ctx.match_state_find(node.id())
        {
            //match_handle_hook(ctx, state, &event)?;
            if let Event::PlayerChat(chat) = &event {
                let account_id = Uuid::parse_str(&chat.account_id).unwrap();
                // Players who joined while the bridge was gone never connected, their chat still counts.
                let user_id = ctx.user_id_or_insert(account_id)?;
                ctx.match_chat_insert(MatchChat::new(
                    state.match_id,
                    state.status(),
                    user_id,
                    chat.text.clone(),
                ))?;
            }
            if state.is_live() {
                handle_match_event(ctx, state, event)?;
            }
        }

        if node.is_server() {
            //TODO handle server events.
        }
    }
    Ok(())
}
