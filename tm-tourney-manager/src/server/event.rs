use spacetimedb::{ReducerContext, Table, table};
use tm_server_types::event::Event;

use crate::{
    auth::Authorization,
    r#match::{
        event::{MatchEvent, match_event},
        match_state::MatchState,
        tm_match,
    },
    server::tm_server,
};

/// Servers call this to post the event stream.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn post_event(ctx: &ReducerContext, event: Event) -> Result<(), String> {
    let login = ctx.auth_server()?;

    if let Some(mut tm_server) = ctx.db.tm_server().tm_login().find(login)
        && let Some(match_id) = tm_server.active_match()
        && let Some(mut tm_match) = ctx.db.tm_match().id().find(match_id)
        && tm_match.is_live()
    {
        let match_changed = tm_match.add_server_event(&event);
        let server_changed = tm_server.add_server_event(&event);

        let match_ended = if let Event::EndMatchEnd(_) = &event {
            log::error!("MATCH ENDED");

            tm_match.end_match();
            tm_server.release();
            true
        } else {
            false
        };

        let tournament_id = tm_match.get_tournament();
        ctx.db.match_event().insert(MatchEvent {
            tournament_id,
            match_id,
            event,
            state: tm_match.get_match_state(),
        });

        if match_changed || match_ended {
            ctx.db.tm_match().id().update(tm_match);
        }
        if server_changed || match_ended {
            ctx.db.tm_server().tm_login().update(tm_server);
        }
    }
    Ok(())
}
