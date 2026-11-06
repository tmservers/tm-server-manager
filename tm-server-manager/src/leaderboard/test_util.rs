use spacetimedb::{ReducerContext, Table};

use crate::{
    leaderboard::{LeaderboardStatus, LeaderboardV3, tab_leaderboard_v3},
    pipeline::LbSettingsV3,
};

/// Inserts a leaderboard which got configured with the steps.
pub(crate) fn configured_leaderboard(ctx: &ReducerContext, id: u32, settings: Vec<LbSettingsV3>) {
    ctx.db.tab_leaderboard_v3().insert(LeaderboardV3 {
        name: String::new(),
        settings,
        id,
        parent_id: 1,
        template: false,
        status: LeaderboardStatus::Configured,
    });
}
