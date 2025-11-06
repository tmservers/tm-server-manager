use spacetimedb::{ReducerContext, Table, table};
use tm_server_types::event::Event;

use crate::{
    r#match::{ephemeral_state::EphemeralState, tm_match},
    server::tm_server,
};

#[table(name = tm_server_event,public)]
pub struct TmServerEvent {
    #[auto_inc]
    #[primary_key]
    id: u64,

    tournament_id: u64,
    match_id: u64,

    state: EphemeralState,

    event: Event,
}

// TODO: remove the id argument and get it from calling entity.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn post_event(ctx: &ReducerContext, id: String, event: Event) {
    if let Some(server) = ctx.db.tm_server().id().find(id)
        && let Some(match_id) = server.active_match()
        && let Some(mut tm_match) = ctx.db.tm_match().id().find(match_id)
        && tm_match.is_live()
    {
        let changed = tm_match.add_server_event(&event);

        let tournament_id = tm_match.get_tournament();

        ctx.db.tm_server_event().insert(TmServerEvent {
            id: 0,
            tournament_id,
            match_id,
            event,
            state: tm_match.get_ephemeral_state(),
        });
        if changed {
            ctx.db.tm_match().id().update(tm_match);
        }
    }
}
