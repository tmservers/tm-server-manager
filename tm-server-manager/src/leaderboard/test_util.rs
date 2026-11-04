use spacetimedb::test_utils::{TestAuth, TestContext};
use tm_server_types::config::TmMode;

use crate::{
    competition::node::NodeHandle,
    leaderboard::{LbEntry, LbEntryKind},
    tm_match::state::{MatchState, MatchStateWrite},
};

pub(crate) const MATCH: NodeHandle = NodeHandle::MatchV1(1);

/// A datastore with the matches being played in the modes.
pub(crate) fn test_context(matches: &[(u32, TmMode)]) -> TestContext {
    let test = TestContext::new().expect("the test datastore should initialize");
    test.with_reducer_tx(TestAuth::internal(), |ctx| {
        for &(match_id, mode) in matches {
            ctx.match_state_insert(MatchState::new(match_id, mode))?;
        }
        Ok::<_, String>(())
    })
    .expect("the matches should be inserted");
    test
}

pub(crate) fn round(user_id: u32, round: u16, score: i32, time: i32) -> LbEntry {
    round_in(MATCH, user_id, round, score, time)
}

pub(crate) fn round_in(
    node: NodeHandle,
    user_id: u32,
    round: u16,
    score: i32,
    time: i32,
) -> LbEntry {
    LbEntry::new(user_id, LbEntryKind::Round, 0, node)
        .set_round(round)
        .set_score(score)
        .set_time(time)
}

pub(crate) fn standing(user_id: u32, position: u16) -> LbEntry {
    LbEntry::new(user_id, LbEntryKind::Standing, position, MATCH)
}

/// The users with their positions, in the order of the entries.
pub(crate) fn ranking(entries: &[LbEntry]) -> Vec<(u32, u16)> {
    entries.iter().map(|e| (e.user_id, e.position)).collect()
}
