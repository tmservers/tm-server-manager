use spacetimedb::{AnonymousViewContext, SpacetimeType, rand::seq::index, table, view};
use tm_server_types::event::Event;

use crate::r#match::match_state::MatchState;

#[table(
    name = tab_tm_match_event,
    index(
        name = event_match,
        btree(columns = [match_id]))
    )]
pub struct TmMatchEvent {
    pub(crate) tournament_id: u32,
    pub(crate) match_id: u32,

    pub(crate) state: MatchState,

    #[index(btree)]
    pub(crate) event: Event,
}

#[derive(Debug, SpacetimeType)]
struct LeaderboardEntry {
    account_id: String,
    score: u32,
}

//TODO this would need a NodeKindeHandle as entry
#[view(name=leaderbaord,public)]
pub fn leaderboard(ctx: &AnonymousViewContext) -> Vec<LeaderboardEntry> {
    /* let entries = Vec::with_capacity(8);
    for event in ctx.db.tab_tm_match_event().event_match().filter(1) {
        entires.LeaderboardEntry { score: 1 }
    } */
    Vec::new()
}
