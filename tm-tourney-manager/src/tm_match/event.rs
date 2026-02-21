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
    //pub(crate) tournament_id: u32,
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

#[derive(Debug, SpacetimeType)]
pub struct LeaderboardEntry {
    account_id: String,
    account_name: String,
    score: i32,
}

//TODO this would need a NodeKindeHandle as entry
#[view(accessor=match_leaderbaord,public)]
pub fn match_leaderboard(ctx: &AnonymousViewContext) -> Vec<LeaderboardEntry> {
    /* let entries = Vec::with_capacity(8);
    for event in ctx.db.tab_tm_match_event().match_id().filter(1) {
        entires.LeaderboardEntry { score: 1 }
    } */
    Vec::new()
}

#[derive(Debug, SpacetimeType)]
pub struct RoundStandings {
    account_id: String,
    account_name: String,
    score: i32,
}

/// If round 0 is supplied we take the current round.
#[view(accessor=match_round,public)]
pub fn match_round(ctx: &AnonymousViewContext) -> Vec<RoundStandings> {
    let match_id = 1u32;
    let mut round = 0u16;

    if round == 0 {
        let Some(state) = ctx.db.tab_tm_match_state().id().find(match_id) else {
            return Vec::new();
        };
        round = state.round;
    }

    //let entries = Vec::with_capacity(8);
    //let players = Hash
    for event in ctx
        .db
        .tab_tm_match_event()
        .match_round_wu()
        .filter((match_id, round, false))
    {
        log::error!("{event:?}");

        match event.event {
            //Event::StartLine()
            _ => continue,
        }
    }
    Vec::new()
}
