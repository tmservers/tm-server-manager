//! The order of the races of a round every mode shares, without ties.

use std::cmp::Reverse;

use crate::RaceResult;

/// The race of a round that counts for a player: the last one in single attempt
/// rounds, the best one in time attack rounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountedRace {
    Last,
    Best,
}

impl CountedRace {
    /// The time of the race that counts, None without a finish.
    pub fn time(self, race: &RaceResult) -> Option<u32> {
        match self {
            CountedRace::Last => race.previous_time,
            CountedRace::Best => race.best_time,
        }
    }

    /// The race times at the checkpoints of the race that counts, or at the
    /// checkpoints a player without a finish passed in their last race.
    fn checkpoint_times(self, race: &RaceResult) -> &[u32] {
        match self {
            _ if self.time(race).is_none() => &race.progress_times,
            CountedRace::Last => &race.previous_checkpoints,
            CountedRace::Best => &race.best_checkpoints,
        }
    }
}

/// Orders players by their races in the round, the best first and without ties:
/// finishers by the time of the race that counts, then everyone else by the
/// checkpoints they passed in their last race, and players without a race in
/// `results` last. Equal races go to the better race time at the last checkpoint,
/// then at the one before and so on. Races that are equal all the way go by name,
/// the case aside, and then by account.
pub fn race_order<'a>(
    players: impl IntoIterator<Item = &'a str>,
    results: &[RaceResult],
    counted: CountedRace,
) -> Vec<String> {
    let mut order: Vec<&str> = players.into_iter().collect();
    order.sort_by_cached_key(|&account_id| {
        let race = results
            .iter()
            .find(|r| r.account_id == account_id && r.started);
        let time = race.and_then(|r| counted.time(r));
        // From the last checkpoint backwards.
        let checkpoint_times: Vec<u32> = race.map_or_else(Vec::new, |r| {
            counted.checkpoint_times(r).iter().rev().copied().collect()
        });
        let race = (
            time.is_none(),
            time,
            Reverse(race.map(|r| r.progress)),
            checkpoint_times,
        );
        (race, by_name(account_id))
    });
    order.into_iter().map(String::from).collect()
}

/// Orders players by name, the case aside, and players with the same name by their
/// account, so nobody ties.
pub(crate) fn by_name(account_id: &str) -> (String, String) {
    (
        crate::name(account_id).to_lowercase(),
        account_id.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{User, testing::native};

    use super::*;

    fn finish(account_id: &str, checkpoints: &[u32]) -> RaceResult {
        let time = checkpoints.last().copied();
        RaceResult {
            account_id: account_id.into(),
            started: true,
            progress: checkpoints.len() as u32,
            progress_times: checkpoints.to_vec(),
            previous_time: time,
            previous_checkpoints: checkpoints.to_vec(),
            best_time: time,
            best_checkpoints: checkpoints.to_vec(),
        }
    }

    /// Gave up after passing checkpoints at these race times.
    fn dnf(account_id: &str, checkpoints: &[u32]) -> RaceResult {
        RaceResult {
            previous_time: None,
            previous_checkpoints: vec![],
            best_time: None,
            best_checkpoints: vec![],
            ..finish(account_id, checkpoints)
        }
    }

    fn order(results: &[RaceResult], counted: CountedRace) -> Vec<String> {
        race_order(
            results.iter().map(|r| r.account_id.as_str()),
            results,
            counted,
        )
    }

    #[test]
    fn finishers_come_first_by_time() {
        let results = [
            dnf("far", &[10_000, 20_000]),
            finish("slow", &[11_000, 21_000, 31_000]),
            RaceResult {
                started: false,
                ..dnf("idle", &[])
            },
            finish("fast", &[10_000, 20_000, 30_000]),
            dnf("near", &[10_000]),
        ];
        assert_eq!(
            order(&results, CountedRace::Last),
            ["fast", "slow", "far", "near", "idle"]
        );
        // Players without a result did not start either.
        assert_eq!(
            race_order(["gone", "fast"], &results, CountedRace::Last),
            ["fast", "gone"]
        );
    }

    #[test]
    fn equal_times_go_to_the_better_checkpoints_from_the_last_one_on() {
        let results = [
            finish("a", &[10_000, 20_500, 30_000]),
            finish("b", &[10_100, 20_400, 30_000]),
            finish("c", &[9_900, 20_400, 30_000]),
        ];
        assert_eq!(order(&results, CountedRace::Last), ["c", "b", "a"]);
    }

    #[test]
    fn players_who_got_equally_far_go_by_when_they_got_there() {
        let results = [dnf("a", &[10_000, 21_000]), dnf("b", &[10_500, 20_000])];
        assert_eq!(order(&results, CountedRace::Last), ["b", "a"]);
    }

    #[test]
    fn the_counted_race_decides() {
        // b drove the best race, a the better last one.
        let results = [
            RaceResult {
                best_time: Some(29_000),
                best_checkpoints: vec![14_000, 29_000],
                ..finish("b", &[16_000, 31_000])
            },
            finish("a", &[15_000, 30_000]),
        ];
        assert_eq!(order(&results, CountedRace::Last), ["a", "b"]);
        assert_eq!(order(&results, CountedRace::Best), ["b", "a"]);
    }

    #[test]
    fn equal_races_go_by_name() {
        native::set(
            0,
            BTreeMap::from([
                (
                    "x".to_string(),
                    User {
                        account_id: "x".into(),
                        name: "bob".into(),
                        club_tag: String::new(),
                        zone: String::new(),
                    },
                ),
                (
                    "y".to_string(),
                    User {
                        account_id: "y".into(),
                        name: "Alice".into(),
                        club_tag: String::new(),
                        zone: String::new(),
                    },
                ),
            ]),
        );
        let race = [10_000, 20_000];
        let results = [finish("x", &race), finish("y", &race), finish("w", &race)];
        // The account stands in for an unknown name.
        assert_eq!(order(&results, CountedRace::Last), ["y", "x", "w"]);
        native::set(0, BTreeMap::new());
    }
}
