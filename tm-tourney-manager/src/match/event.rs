use spacetimedb::{rand::seq::index, table};
use tm_server_types::event::Event;

use crate::r#match::match_state::MatchState;

#[table(
    name = match_event,
    public,
    index(
        name = event_match,
        btree(columns = [match_id]))
    )]
pub struct MatchEvent {
    pub(crate) tournament_id: u32,
    pub(crate) match_id: u32,

    pub(crate) state: MatchState,

    #[index(btree)]
    pub(crate) event: Event,
}
