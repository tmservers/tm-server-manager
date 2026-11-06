use std::{cmp::Reverse, collections::HashSet};

use tm_server_types::config::{ModeSettingsV2, TmMode};

use crate::{
    competition::node::NodeHandle,
    leaderboard::{
        LbEntry, LbEntryKind, LbParams, LbParticipant, combinators::LbRows, context::LbContext,
        error::LbError, shared_map,
    },
};

/// How the rounds of a match become its standings.
///
/// Standings are computed again whenever they are read, also for matches which are long over.
/// So a mode must never change how it ranks, a change needs a version stored with the match.
/// E.g. a column on the match state whose default is the version before the change, so existing matches keep it.
pub(crate) trait ModeRules {
    fn standings(
        &self,
        cx: &LbContext,
        node: NodeHandle,
        rounds: Vec<LbEntry>,
    ) -> Result<Vec<LbEntry>, LbError>;
}

impl ModeRules for TmMode {
    fn standings(
        &self,
        cx: &LbContext,
        node: NodeHandle,
        rounds: Vec<LbEntry>,
    ) -> Result<Vec<LbEntry>, LbError> {
        Ok(match self {
            TmMode::Rounds => self::rounds(rounds),
            TmMode::ReverseCup => reverse_cup(reverse_cup_starting_points(cx, node)?, rounds),
            TmMode::Knockout => knockout(rounds),
            TmMode::TimeAttack => time_attack(rounds),
            TmMode::Unknown => by_position(rounds),
        })
    }
}

/// The standings of every match in the entries, one after another in the order the matches first appear.
/// Entries which already are standings are only sorted, so applying this again changes nothing.
pub(crate) fn match_standings(
    cx: &LbContext,
    entries: Vec<LbEntry>,
) -> Result<Vec<LbEntry>, LbError> {
    Ok(entries
        .group_by(|e| (e.get_node(), e.kind))
        .try_map(|(node, kind), entries| match kind {
            LbEntryKind::Round => round_standings(cx, node, entries),
            LbEntryKind::Standing => Ok(by_position(entries)),
        })?
        .flatten())
}

/// The rounds of a single match, its mode decides how they become standings.
fn round_standings(
    cx: &LbContext,
    node: NodeHandle,
    rounds: Vec<LbEntry>,
) -> Result<Vec<LbEntry>, LbError> {
    let Some(mode) = cx.match_mode(node) else {
        return Err(LbError::new(format!(
            "The rounds of {node:?} have no match mode."
        )));
    };

    let mut standings = mode.standings(cx, node, rounds)?;
    for standing in &mut standings {
        standing.kind = LbEntryKind::Standing;
    }
    Ok(standings)
}

fn reverse_cup_starting_points(cx: &LbContext, node: NodeHandle) -> Result<i32, LbError> {
    match cx.mode_settings(node) {
        Some(ModeSettingsV2::ReverseCup(reverse_cup)) => Ok(reverse_cup.starting_points),
        _ => Err(LbError::new(format!(
            "Could not find the reverse cup config of {node:?}."
        ))),
    }
}

fn by_position(entries: Vec<LbEntry>) -> Vec<LbEntry> {
    entries.sorted_by_key(|e| e.position)
}

/// Adds up the rounds of a participant and keeps the latest round.
fn summed(mut standing: LbEntry, round: LbEntry) -> LbEntry {
    standing.score = standing.score.saturating_add(round.score);
    standing.round = standing.round.max(round.round);
    standing.map_id = shared_map(standing.map_id, round.map_id);
    standing
}

fn rounds(rounds: Vec<LbEntry>) -> Vec<LbEntry> {
    rounds
        .group_by(|e| e.participant())
        .reduce(summed)
        .sorted_by_key(|s| LbParams::Score.key(s))
        .renumbered()
}

fn reverse_cup(starting_points: i32, rounds: Vec<LbEntry>) -> Vec<LbEntry> {
    let eliminated: HashSet<LbParticipant> = rounds
        .iter()
        .filter(|e| e.score <= -2000)
        .map(|e| e.participant())
        .collect();

    let mut standings = rounds.group_by(|e| e.participant()).reduce(summed);
    for standing in &mut standings {
        standing.score = standing.score.saturating_add(starting_points);

        if !eliminated.contains(&standing.participant()) || standing.score > 0 {
            standing.round += 1;
        }
    }

    // Who is in the same round is ranked by the points left.
    standings
        .sorted_by_key(|s| (Reverse(s.round), Reverse(s.score)))
        .renumbered()
}

fn knockout(rounds: Vec<LbEntry>) -> Vec<LbEntry> {
    rounds
        .group_by(|e| e.participant())
        .reduce(|mut standing, round| {
            let map_id = shared_map(standing.map_id, round.map_id);
            let round_number = standing.round.max(round.round);
            // The round in which the player got knocked out, otherwise the latest one.
            let knocked_out = standing.score == -1;
            if round.score == -1 || (!knocked_out && round.round >= standing.round) {
                standing = round;
            }
            standing.round = round_number;
            standing.map_id = map_id;
            standing
        })
        // Who got knocked out in the same round is ranked by the place in that round.
        .sorted_by_key(|s| (Reverse(s.round), s.position))
        .renumbered()
}

fn time_attack(rounds: Vec<LbEntry>) -> Vec<LbEntry> {
    rounds
        .group_by(|e| e.participant())
        .reduce(|best, round| {
            let map_id = shared_map(best.map_id, round.map_id);
            let mut best = if LbParams::time_key(round.time) < LbParams::time_key(best.time) {
                round
            } else {
                best
            };
            best.map_id = map_id;
            best
        })
        .sorted_by_key(|s| LbParams::Time.key(s))
        .renumbered()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leaderboard::test_util::{MATCH, ranking, round, round_in, test_context};

    #[test]
    fn every_match_becomes_standings_with_its_own_mode() {
        let test = test_context(&[(1, TmMode::Rounds), (2, TmMode::TimeAttack)]);
        let time_attack = NodeHandle::MatchV1(2);

        let standings = match_standings(
            &LbContext::new(&test.anonymous_view_context(), MATCH),
            vec![
                round(1, 1, 10, 3000),
                round_in(time_attack, 1, 1, 20, 3000),
                round(2, 1, 20, 2000),
                round_in(time_attack, 2, 1, 10, 2000),
                round(1, 2, 15, 0),
            ],
        )
        .unwrap();

        // In both matches player 1 has more points and player 2 the better time, so the order shows the mode used.
        let rows: Vec<(NodeHandle, u32, u16)> = standings
            .iter()
            .map(|s| (s.get_node(), s.user_id, s.position))
            .collect();
        assert_eq!(
            rows,
            vec![
                (MATCH, 1, 1),
                (MATCH, 2, 2),
                (time_attack, 2, 1),
                (time_attack, 1, 2)
            ]
        );
        assert!(standings.iter().all(|s| s.kind == LbEntryKind::Standing));
    }

    #[test]
    fn rounds_of_a_match_without_state_are_an_error() {
        let test = test_context(&[]);

        let standings = match_standings(
            &LbContext::new(&test.anonymous_view_context(), MATCH),
            vec![round(1, 1, 10, 0)],
        );

        assert!(standings.is_err());
    }

    #[test]
    fn rounds_ranks_by_the_summed_score() {
        let standings = rounds(vec![
            round(1, 1, 10, 0),
            round(2, 1, 5, 0),
            round(3, 1, 5, 0),
            round(1, 2, 0, 0),
            round(2, 2, 10, 0),
            round(3, 2, 10, 0),
        ]);

        // 2 and 3 are tied, the one which appeared first keeps the better place.
        assert_eq!(ranking(&standings), vec![(2, 1), (3, 2), (1, 3)]);
        let scores: Vec<i32> = standings.iter().map(|s| s.score).collect();
        assert_eq!(scores, vec![15, 15, 10]);
        assert!(standings.iter().all(|s| s.round == 2));
    }

    #[test]
    fn reverse_cup_ranks_players_which_are_still_in_first() {
        let standings = reverse_cup(1000, vec![round(2, 1, -2000, 0), round(1, 1, -500, 0)]);

        // Player 2 got eliminated without points left so they stay in the round they were eliminated in.
        assert_eq!(ranking(&standings), vec![(1, 1), (2, 2)]);
        let rows: Vec<(i32, u16)> = standings.iter().map(|s| (s.score, s.round)).collect();
        assert_eq!(rows, vec![(500, 2), (-1000, 1)]);
    }

    #[test]
    fn reverse_cup_ranks_players_in_the_same_round_by_their_points_left() {
        let standings = reverse_cup(1000, vec![round(1, 1, -500, 0), round(2, 1, -100, 0)]);

        assert_eq!(ranking(&standings), vec![(2, 1), (1, 2)]);
    }

    #[test]
    fn knockout_ranks_by_the_round_a_player_was_knocked_out() {
        let standings = knockout(vec![
            round(1, 1, 0, 0),
            round(2, 1, 0, 0),
            round(3, 1, 0, 0),
            round(1, 2, 0, 0),
            round(2, 2, 0, 0),
            round(3, 2, -1, 0),
            round(1, 3, 0, 0),
            round(2, 3, -1, 0),
            round(1, 4, 0, 0),
        ]);

        assert_eq!(ranking(&standings), vec![(1, 1), (2, 2), (3, 3)]);
        let rounds: Vec<u16> = standings.iter().map(|s| s.round).collect();
        assert_eq!(rounds, vec![4, 3, 2]);
    }

    #[test]
    fn knockout_ranks_players_knocked_out_together_by_their_place_in_that_round() {
        let place = |entry: LbEntry, position| LbEntry { position, ..entry };
        let standings = knockout(vec![
            place(round(1, 1, 0, 0), 1),
            place(round(2, 1, -1, 0), 3),
            place(round(3, 1, -1, 0), 2),
        ]);

        assert_eq!(ranking(&standings), vec![(1, 1), (3, 2), (2, 3)]);
    }

    #[test]
    fn time_attack_keeps_the_best_driven_time() {
        let standings = time_attack(vec![
            round(1, 1, 0, 5000),
            round(2, 1, 0, 0),
            round(1, 2, 0, 0),
            round(3, 2, 0, 4500),
            round(1, 3, 0, 4000),
        ]);

        // Not driving is worse than any time.
        assert_eq!(ranking(&standings), vec![(1, 1), (3, 2), (2, 3)]);
        let times: Vec<i32> = standings.iter().map(|s| s.time).collect();
        assert_eq!(times, vec![4000, 4500, 0]);
    }

    #[test]
    fn standings_keep_the_map_only_if_every_round_was_played_on_it() {
        let standings = rounds(vec![
            round(1, 1, 10, 0).set_map(7),
            round(2, 1, 5, 0).set_map(7),
            round(1, 2, 10, 0).set_map(7),
            round(2, 2, 5, 0).set_map(8),
        ]);

        let maps: Vec<(u32, u32)> = standings.iter().map(|s| (s.user_id, s.map_id)).collect();
        assert_eq!(maps, vec![(1, 7), (2, 0)]);
    }
}
