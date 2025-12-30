use spacetimedb::{ReducerContext, Table, table};
use tm_server_types::event::Event;

use crate::{
    auth::Authorization,
    r#match::{
        event::{TmMatchEvent, tab_tm_match_event},
        match_state::{TmMatchState, tab_tm_match_state},
        tab_tm_match,
    },
    server::tab_tm_server,
};

/// Servers call this to post the event stream.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn post_event(ctx: &ReducerContext, event: Event) -> Result<(), String> {
    let login = ctx.get_server()?;

    if let Some(mut tm_server) = ctx.db.tab_tm_server().tm_login().find(login)
        && let Some(match_id) = tm_server.active_match()
        && let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(match_id)
        && tm_match.is_live()
        && let Some(mut match_state) = ctx.db.tab_tm_match_state().id().find(match_id)
    {
        // TODO maybe move this whole thing in a function for TmMatch to handle it there.

        let match_state_changed = match_state.add_server_event(&event);
        let server_changed = tm_server.add_server_event(&event);

        let match_ended = if let Event::EndMatchEnd(_) = &event {
            log::error!("MATCH ENDED");

            tm_match.end_match();
            ctx.db.tab_tm_match().id().update(tm_match);
            tm_server.release();
            true
        } else {
            false
        };

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
        if server_changed || match_ended {
            ctx.db.tab_tm_server().tm_login().update(tm_server);
        }
    }
    Ok(())
}
