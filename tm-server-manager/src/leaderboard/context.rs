use spacetimedb::{CtxDbRead, LocalReadOnly};
use tm_server_types::config::{ModeSettingsV2, TmMode};

use crate::{
    competition::node::NodeHandle,
    raw_server::config::RawServerContigRead,
    tm_match::{MatchRead, state::MatchStateRead},
};

/// What the steps of a leaderboard can read while they are evaluated for `origin`.
///
/// Steps only read what they need when they need it, so a view evaluating them only depends on those rows.
/// Reads should be point lookups, everything else makes the view depend on the whole table.
/// Nothing read from the database may be kept across evaluations, the view would then miss its changes.
#[derive(Clone)]
pub(crate) struct LbContext {
    db: LocalReadOnly,
    origin: NodeHandle,
}

impl LbContext {
    pub(crate) fn new(ctx: &impl CtxDbRead, origin: NodeHandle) -> Self {
        LbContext {
            db: ctx.db_read_only().clone(),
            origin,
        }
    }

    /// The node the steps are evaluated for, entries created by a step belong to it.
    pub(crate) fn origin(&self) -> NodeHandle {
        self.origin
    }

    /// The mode the match is currently played in.
    pub(crate) fn match_mode(&self, node: NodeHandle) -> Option<TmMode> {
        let NodeHandle::MatchV1(match_id) = node else {
            return None;
        };
        Some(self.match_state_find(match_id)?.get_mode())
    }

    /// The mode settings of the config the match is currently played with.
    pub(crate) fn mode_settings(&self, node: NodeHandle) -> Option<ModeSettingsV2> {
        let NodeHandle::MatchV1(match_id) = node else {
            return None;
        };
        let tm_match = self.match_find(match_id)?;
        let config = self
            .raw_server_config(tm_match.get_active_config_id())
            .ok()?;
        Some(config.get_mode().clone())
    }
}

impl CtxDbRead for LbContext {
    fn db_read_only(&self) -> &LocalReadOnly {
        &self.db
    }
}
