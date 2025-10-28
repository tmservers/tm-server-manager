use tm_server_types::event::Event;

use crate::{r#match::tm_match, server::tm_server};

#[cfg_attr(feature="spacetime", spacetimedb::table(name = tm_server_event,public))]
pub struct TmServerEvent {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    #[cfg_attr(feature = "spacetime", primary_key)]
    id: u64,

    match_id: u64,

    event: Event,
}

// TODO: remove the id argument and get it from calling entity.
#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn post_event(ctx: &spacetimedb::ReducerContext, id: String, event: Event) {
    use spacetimedb::Table;

    if let Some(server) = ctx.db.tm_server().id().find(id)
        && let Some(match_id) = server.active_match()
        && let Some(mut stage_match) = ctx.db.tm_match().id().find(match_id)
        && stage_match.is_live()
    {
        stage_match.add_server_event(&event);

        ctx.db.tm_match().id().update(stage_match);

        ctx.db.tm_server_event().insert(TmServerEvent {
            id: 0,
            match_id,
            event,
        });
    }
}
