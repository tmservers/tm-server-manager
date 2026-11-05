use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    leaderboard::{
        LbEntry, LbEntryKind, LbParams, LbParticipant, LbStep,
        combinators::{Groups, LbRows},
        context::LbContext,
        error::LbError,
        shape::{LbOrigins, LbShape, check_participants},
        shared_map, shared_team,
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

/// Merges all entries of a participant into one, it keeps the map and team only if all of them share it.
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

    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        if input.is_empty() {
            return Ok(input);
        }
        check_participants(&input)?;
        Ok(LbShape {
            rounds: false,
            standings: true,
            origins: LbOrigins::One(origin),
            ..input
        })
    }
}

/// Merges all entries of every team into one standing of the team, like [`LbMergeSettingsV2`] does per participant.
/// Entries of players without a team are left out.
pub(crate) struct LbMergeTeams<'a>(pub(crate) &'a LbMergeSettingsV2);

impl LbStep for LbMergeTeams<'_> {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        Ok(self.0.evaluate_teams(cx.origin(), entries))
    }

    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        Ok(LbShape {
            players: false,
            teams: !input.is_empty(),
            ..self.0.check(origin, input)?
        })
    }
}

impl LbMergeSettingsV2 {
    fn evaluate(self, origin: NodeHandle, leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        self.merge_groups(origin, leaderboard.group_by(|e| e.participant()))
    }

    fn evaluate_teams(self, origin: NodeHandle, leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        let with_team: Vec<LbEntry> = leaderboard.into_iter().filter(|e| e.team_id != 0).collect();
        self.merge_groups(
            origin,
            with_team.group_by(|e| LbParticipant::Team(e.team_id)),
        )
    }

    fn merge_groups(
        self,
        origin: NodeHandle,
        groups: Groups<LbParticipant, LbEntry>,
    ) -> Vec<LbEntry> {
        let merged = groups.fold(|participant, entries| {
            let map_id = entries.iter().map(|e| e.map_id).reduce(shared_map);
            let team_id = entries.iter().map(|e| e.team_id).reduce(shared_team);
            let mut init = LbEntry::for_participant(participant, LbEntryKind::Standing, 0, origin)
                .set_score(0)
                .set_time(0)
                .set_map(map_id.unwrap_or(0));
            init.team_id = team_id.unwrap_or(0);
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
    use crate::leaderboard::{
        shape::LbOrigins,
        test_util::{for_team, ranking, round, round_in, team_standing},
    };

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

    #[test]
    fn teams_without_users_are_merged_per_team() {
        let merged = merge(
            LbMergeAction::Summate,
            LbParams::Score,
            vec![
                team_standing(1, 1, 10),
                team_standing(2, 2, 30),
                team_standing(1, 1, 25),
            ],
        );

        let rows: Vec<(u32, u32, i32)> = merged
            .iter()
            .map(|e| (e.user_id, e.team_id, e.score))
            .collect();
        assert_eq!(rows, vec![(0, 1, 35), (0, 2, 30)]);
    }

    #[test]
    fn merged_players_keep_their_team_only_if_they_always_played_for_it() {
        let other = NodeHandle::MatchV1(2);
        let merged = merge(
            LbMergeAction::Summate,
            LbParams::Score,
            vec![
                for_team(round(1, 1, 10, 0), 5),
                for_team(round(2, 1, 20, 0), 5),
                for_team(round_in(other, 1, 1, 10, 0), 5),
                for_team(round_in(other, 2, 1, 20, 0), 6),
            ],
        );

        let teams: Vec<(u32, u32)> = merged.iter().map(|e| (e.user_id, e.team_id)).collect();
        assert_eq!(teams, vec![(2, 0), (1, 5)]);
    }

    #[test]
    fn merged_entries_of_several_origins_belong_to_the_merging_node() {
        let rounds = LbShape::rounds(NodeHandle::MatchV1(1))
            .combine(LbShape::rounds(NodeHandle::MatchV1(2)));

        let merged = LbMergeSettingsV2 {
            action: LbMergeAction::Summate,
            param: LbParams::Score,
        }
        .check(LEADERBOARD, rounds)
        .unwrap();

        assert_eq!(merged.origins, LbOrigins::One(LEADERBOARD));
        assert!(merged.standings && !merged.rounds);
    }
}
