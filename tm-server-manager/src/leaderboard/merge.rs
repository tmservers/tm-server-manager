use spacetimedb::SpacetimeType;
use tm_server_types::config::TmMode;

use crate::{
    competition::node::NodeHandle,
    leaderboard::{LbEntry, LbParams, group_by, renumber_positions},
};

/// Superseded by [`LbMergeSettingsV2`].
#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbMergeSettings {
    kind: LbMergeKind,
    action: LbMergeAction,
    param: LbParams,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbMergeKind {
    Rounds,
}

impl From<LbMergeSettings> for LbMergeSettingsV2 {
    fn from(settings: LbMergeSettings) -> Self {
        LbMergeSettingsV2 {
            action: settings.action,
            param: settings.param,
        }
    }
}

/// Merges all entries of a user into one.
#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbMergeSettingsV2 {
    action: LbMergeAction,
    param: LbParams,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbMergeAction {
    Average,
    Summate,
}

impl LbMergeSettingsV2 {
    pub(super) fn evaluate(self, origin: NodeHandle, leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        let mut merged: Vec<LbEntry> = group_by(leaderboard, |e| e.user_id)
            .into_iter()
            .map(|(player, entries)| {
                let init = LbEntry::new(player, TmMode::Unknown, 0, origin)
                    .set_score(0)
                    .set_time(0);
                self.merge(init, &entries)
            })
            .collect();
        log::info!("Merge Result: {:?}", merged);

        self.param.sort(&mut merged);
        // When merging positions they are what gets sorted by so they are kept.
        if !matches!(self.param, LbParams::Position) {
            renumber_positions(&mut merged);
        }

        merged
    }

    fn merge(self, mut merged: LbEntry, entries: &[LbEntry]) -> LbEntry {
        match self.param {
            LbParams::Score => {
                let sum = entries.iter().map(|e| e.score as i64).sum();
                merged.score = self
                    .combine(sum, entries.len())
                    .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            }
            LbParams::Time => {
                // A single entry which was not driven makes the merged one not driven aswell.
                merged.time = if entries.iter().any(|e| e.time <= 0) {
                    0
                } else {
                    let sum = entries.iter().map(|e| e.time as i64).sum();
                    self.combine(sum, entries.len()).min(i32::MAX as i64) as i32
                };
            }
            LbParams::Position => {
                let sum = entries.iter().map(|e| e.position as i64).sum();
                merged.position = self.combine(sum, entries.len()).min(u16::MAX as i64) as u16;
            }
        }
        merged
    }

    fn combine(self, sum: i64, count: usize) -> i64 {
        match self.action {
            LbMergeAction::Summate => sum,
            LbMergeAction::Average => sum / count as i64,
        }
    }
}
