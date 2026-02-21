use spacetimedb::{AnonymousViewContext, SpacetimeType, rand::seq::index, table, view};
use tm_server_types::event::Event;

use crate::tm_match::match_state::{TmMatchState, tab_tm_match_state__view};

#[derive(Debug)]
#[table(
    accessor = tab_tm_match_event,
    index(
        accessor = match_round_wu,
        btree(columns = [match_id,round,is_warmup]))
    )]
pub struct TmMatchEvent {
    //pub(crate) project_id: u32,
    #[index(btree)]
    pub(crate) match_id: u32,

    /// This is mirrored from match state
    /// splatted for querying
    pub(crate) restarted: u16,
    pub(crate) round: u16,
    pub(crate) warmup: u16,
    pub(crate) is_warmup: bool,
    pub(crate) paused: bool,
    ///Unitl here

    #[index(btree)]
    pub(crate) event: Event,
}
