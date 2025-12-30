use spacetimedb::{AnonymousViewContext, SpacetimeType, rand::seq::index, table, view};
use tm_server_types::event::Event;

#[derive(Debug)]
#[table(name = tab_tm_match_players)]
#[table(name = tab_tm_match_spectators)]
pub struct TmMatchPlayer {
    #[index(btree)]
    pub(crate) match_id: u32,

    #[unique]
    pub(crate) account_id: String,
}

#[view(name= match_players,public)]
pub fn match_players(ctx: &AnonymousViewContext) -> Vec<TmMatchPlayer> {
    Vec::new()
}
