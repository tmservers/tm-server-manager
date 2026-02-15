use spacetimedb::{ReducerContext, Table, Uuid, reducer, table};
use tm_server_types::event::Event;

use crate::{
    authorization::Authorization,
    competition::connection::{NodeKindHandle, internal_graph_resolution_node_finished},
    r#match::{
        event::{TmMatchEvent, tab_tm_match_event},
        match_state::{TmMatchState, tab_tm_match_state},
        players::{TmMatchPlayer, tab_tm_match_players, tab_tm_match_spectators},
        tab_tm_match,
    },
    raw_server::tab_raw_server,
};

/// Servers call this to post the event stream.
#[reducer]
pub fn post_event(ctx: &ReducerContext, event: Event) -> Result<(), String> {
    let mut server = ctx.get_server()?;

    if let Some(match_id) = server.active_match()
        && let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(match_id)
        && tm_match.is_live()
        && let Some(mut match_state) = ctx.db.tab_tm_match_state().id().find(match_id)
    {
        // TODO maybe move this whole thing in a function for TmMatch to handle it there.

        let match_state_changed = match_state.add_server_event(&event);
        //let server_changed = tm_server.add_server_event(&event);

        let match_ended = if let Event::EndMatchEnd(_) = &event {
            log::error!("MATCH ENDED");

            tm_match.end_match();
            let tm_match = ctx.db.tab_tm_match().id().update(tm_match);

            internal_graph_resolution_node_finished(
                ctx,
                tm_match.get_comp_id(),
                NodeKindHandle::MatchV1(tm_match.id),
            )?;

            server.release();
            true
        } else {
            false
        };

        match &event {
            Event::PlayerConenct(player) => {
                match player.is_spectator {
                    true => ctx
                        .db
                        .tab_tm_match_players()
                        .account_id()
                        .try_insert_or_update(TmMatchPlayer {
                            match_id,
                            account_id: Uuid::parse_str(&player.account_id).unwrap(),
                        })?,
                    false => ctx
                        .db
                        .tab_tm_match_spectators()
                        .account_id()
                        .try_insert_or_update(TmMatchPlayer {
                            match_id,
                            account_id: Uuid::parse_str(&player.account_id).unwrap(),
                        })?,
                };
            }
            Event::PlayerDisconnect(player) => {
                if !ctx
                    .db
                    .tab_tm_match_players()
                    .account_id()
                    .delete(Uuid::parse_str(&player.account_id).unwrap())
                {
                    ctx.db
                        .tab_tm_match_spectators()
                        .account_id()
                        .delete(Uuid::parse_str(&player.account_id).unwrap());
                }
            }
            _ => (),
        }

        //let tournament_id = tm_match.get_tournament();
        ctx.db.tab_tm_match_event().insert(TmMatchEvent {
            //tournament_id,
            match_id,
            event,
            restarted: match_state.restarted,
            round: match_state.round,
            warmup: match_state.warmup,
            is_warmup: match_state.is_warmup,
            paused: match_state.paused,
        });

        if match_state_changed || match_ended {
            ctx.db.tab_tm_match_state().id().update(match_state);
        }
        if match_ended {
            ctx.db.tab_raw_server().server_login().update(server);
        }
    }
    Ok(())
}
