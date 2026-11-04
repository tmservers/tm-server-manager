use spacetimedb::test_utils::{TestAuth, TestContext};
use tm_server_types::config::TmMode;

use crate::tm_match::state::{MatchState, MatchStateWrite};

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
