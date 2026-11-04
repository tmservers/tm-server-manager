use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    leaderboard::{
        LbEntry, LbEntryKind, LbParams, LbStep, combinators::LbRows, context::LbContext,
        error::LbError, shared_map,
    },
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

/// Merges all entries of a participant into one.
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

impl LbStep for LbMergeSettingsV2 {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        Ok(self.evaluate(cx.origin(), entries))
    }
}

impl LbMergeSettingsV2 {
    fn evaluate(self, origin: NodeHandle, leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        let merged = leaderboard
            .group_by(|e| e.participant())
            .fold(|participant, entries| {
                let map_id = entries.iter().map(|e| e.map_id).reduce(shared_map);
                let init = LbEntry::for_participant(participant, LbEntryKind::Standing, 0, origin)
                    .set_score(0)
                    .set_time(0)
                    .set_map(map_id.unwrap_or(0));
                self.merge(init, &entries)
            });
        log::info!("Merge Result: {:?}", merged);

        let merged = merged.sorted_by_key(|e| self.param.key(e));
        // When merging positions they are what gets sorted by so they are kept.
        match self.param {
            LbParams::Position => merged,
            _ => merged.renumbered(),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leaderboard::test_util::{ranking, round};

    const LEADERBOARD: NodeHandle = NodeHandle::LeaderboardV1(7);

    fn merge(action: LbMergeAction, param: LbParams, entries: Vec<LbEntry>) -> Vec<LbEntry> {
        LbMergeSettingsV2 { action, param }.evaluate(LEADERBOARD, entries)
    }

    #[test]
    fn summed_scores_become_standings_of_the_leaderboard() {
        let merged = merge(
            LbMergeAction::Summate,
            LbParams::Score,
            vec![round(1, 1, 10, 0), round(2, 1, 30, 0), round(1, 2, 25, 0)],
        );

        assert_eq!(ranking(&merged), vec![(1, 1), (2, 2)]);
        let scores: Vec<i32> = merged.iter().map(|e| e.score).collect();
        assert_eq!(scores, vec![35, 30]);
        assert!(merged.iter().all(|e| e.get_node() == LEADERBOARD));
        assert!(merged.iter().all(|e| e.kind == LbEntryKind::Standing));
    }

    #[test]
    fn an_undriven_time_makes_the_average_undriven() {
        let merged = merge(
            LbMergeAction::Average,
            LbParams::Time,
            vec![
                round(2, 1, 0, 1000),
                round(1, 1, 0, 1000),
                round(2, 2, 0, 0),
                round(1, 2, 0, 3000),
            ],
        );

        assert_eq!(ranking(&merged), vec![(1, 1), (2, 2)]);
        let times: Vec<i32> = merged.iter().map(|e| e.time).collect();
        assert_eq!(times, vec![2000, 0]);
    }

    #[test]
    fn merged_positions_are_kept_instead_of_renumbered() {
        let entries = [(1, 1), (2, 2), (3, 4), (1, 3), (2, 2), (3, 6)].map(|(user, position)| {
            LbEntry::new(user, LbEntryKind::Standing, position, LEADERBOARD)
        });

        let merged = merge(LbMergeAction::Average, LbParams::Position, entries.to_vec());

        assert_eq!(ranking(&merged), vec![(1, 2), (2, 2), (3, 5)]);
    }
}
