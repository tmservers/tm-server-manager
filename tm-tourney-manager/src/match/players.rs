use spacetimedb::{AnonymousViewContext, SpacetimeType, rand::seq::index, table, view};
use tm_server_types::event::Event;

#[derive(Debug)]
#[table(name = tab_tm_match_players)]
//#[table(name = tab_tm_match_spectators)]
pub struct TmMatchPlayer {
    #[index(btree)]
    pub(crate) match_id: u32,

    #[unique]
    account_id: String,
}
