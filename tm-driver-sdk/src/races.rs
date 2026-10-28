//! The races of a round, which of them counts and their order every mode shares,
//! without ties.

use std::cmp::Reverse;

use crate::{Race, Racer, RoundEnd};

impl RoundEnd {
    /// The races of a player in the round, None if the player did not start.
    pub fn racer(&self, account_id: &str) -> Option<&Racer> {
        self.racers.iter().find(|r| r.account_id == account_id)
    }
}

impl Race {
    /// The race time at the finish, None without a finish.
    pub fn time(&self) -> Option<u32> {
        if self.finished {
            self.checkpoints.last().copied()
        } else {
            None
        }
    }
}

/// The race of a round that counts for a player: the last one in single attempt
/// rounds, the best one in time attack rounds. Only finished races count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountedRace {
    /// The last finished race.
    Last,
    /// The fastest finished race, the earlier one of equal times.
    Best,
}

impl CountedRace {
    /// The race that counts, None without a finish.
    pub fn race(self, racer: &Racer) -> Option<&Race> {
        let mut finished = racer.races.iter().filter(|race| race.time().is_some());
        match self {
            CountedRace::Last => finished.next_back(),
            CountedRace::Best => finished.min_by_key(|race| race.time()),
        }
    }

    /// The time of the race that counts, None without a finish.
    pub fn time(self, racer: &Racer) -> Option<u32> {
        self.race(racer).and_then(Race::time)
    }
}

/// Orders players by their races in the round, the best first and without ties:
/// finishers by the time of the race that counts, then everyone else by the
/// checkpoints they passed in their last race, and players who did not start
/// last. Equal races go to the better race time at the last checkpoint, then at
/// the one before and so on. Races that are equal all the way go by account, not
/// by name: names are only for display, a replay may not know them.
pub fn race_order<'a>(
    players: impl IntoIterator<Item = &'a str>,
    round_end: &RoundEnd,
    counted: CountedRace,
) -> Vec<String> {
    let mut order: Vec<&str> = players.into_iter().collect();
    order.sort_by_cached_key(|&account_id| {
        // The race that counts, the last one without a finish.
        let race = round_end
            .racer(account_id)
            .and_then(|racer| counted.race(racer).or(racer.races.last()));
        let time = race.and_then(Race::time);
        // From the last checkpoint backwards.
        let checkpoint_times: Vec<u32> = race.map_or_else(Vec::new, |race| {
            race.checkpoints.iter().rev().copied().collect()
        });
        (
            time.is_none(),
            time,
            Reverse(race.map(|race| race.checkpoints.len())),
            checkpoint_times,
            account_id,
        )
    });
    order.into_iter().map(String::from).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{User, testing::native};

    use super::*;

    fn race(checkpoints: &[u32], finished: bool) -> Race {
        Race {
            checkpoints: checkpoints.to_vec(),
            finished,
        }
    }

    fn racer(account_id: &str, races: Vec<Race>) -> Racer {
        Racer {
            account_id: account_id.into(),
            races,
        }
    }

    /// Finished with these race times at the checkpoints, the finish last.
    fn finish(account_id: &str, checkpoints: &[u32]) -> Racer {
        racer(account_id, vec![race(checkpoints, true)])
    }

    /// Gave up after passing checkpoints at these race times.
    fn dnf(account_id: &str, checkpoints: &[u32]) -> Racer {
        racer(account_id, vec![race(checkpoints, false)])
    }

    fn round_end(racers: Vec<Racer>) -> RoundEnd {
        RoundEnd {
            invalid: false,
            racers,
            present: Vec::new(),
        }
    }

    fn order(players: &[&str], round_end: &RoundEnd, counted: CountedRace) -> Vec<String> {
        race_order(players.iter().copied(), round_end, counted)
    }

    #[test]
    fn finishers_come_first_by_time() {
        let round_end = round_end(vec![
            dnf("far", &[10_000, 20_000]),
            finish("slow", &[11_000, 21_000, 31_000]),
            // Gave up before the first checkpoint.
            dnf("start", &[]),
            finish("fast", &[10_000, 20_000, 30_000]),
            dnf("near", &[10_000]),
        ]);
        // idle did not start.
        assert_eq!(
            order(
                &["far", "slow", "idle", "start", "fast", "near"],
                &round_end,
                CountedRace::Last
            ),
            ["fast", "slow", "far", "near", "start", "idle"]
        );
    }

    #[test]
    fn equal_times_go_to_the_better_checkpoints_from_the_last_one_on() {
        let round_end = round_end(vec![
            finish("a", &[10_000, 20_500, 30_000]),
            finish("b", &[10_100, 20_400, 30_000]),
            finish("c", &[9_900, 20_400, 30_000]),
        ]);
        assert_eq!(
            order(&["a", "b", "c"], &round_end, CountedRace::Last),
            ["c", "b", "a"]
        );
    }

    #[test]
    fn players_who_got_equally_far_go_by_when_they_got_there() {
        let round_end = round_end(vec![
            dnf("a", &[10_000, 21_000]),
            dnf("b", &[10_500, 20_000]),
        ]);
        assert_eq!(
            order(&["a", "b"], &round_end, CountedRace::Last),
            ["b", "a"]
        );
    }

    #[test]
    fn the_counted_race_decides() {
        // b drove the best race, a the better last one.
        let round_end = round_end(vec![
            racer(
                "b",
                vec![race(&[14_000, 29_000], true), race(&[16_000, 31_000], true)],
            ),
            finish("a", &[15_000, 30_000]),
        ]);
        assert_eq!(
            order(&["a", "b"], &round_end, CountedRace::Last),
            ["a", "b"]
        );
        assert_eq!(
            order(&["a", "b"], &round_end, CountedRace::Best),
            ["b", "a"]
        );
    }

    #[test]
    fn only_finished_races_count() {
        let racer = racer(
            "a",
            vec![
                race(&[15_000, 31_000], true),
                race(&[14_000, 30_000], true),
                race(&[13_000, 30_000], true),
                // Restarted after the first checkpoint.
                race(&[12_000], false),
            ],
        );
        assert_eq!(CountedRace::Last.race(&racer), Some(&racer.races[2]));
        // The earlier one of equal times.
        assert_eq!(CountedRace::Best.race(&racer), Some(&racer.races[1]));
        assert_eq!(CountedRace::Best.time(&racer), Some(30_000));

        let gave_up = dnf("b", &[10_000]);
        assert_eq!(CountedRace::Last.race(&gave_up), None);
        assert_eq!(CountedRace::Best.time(&gave_up), None);
    }

    #[test]
    fn players_without_a_finish_go_by_their_last_race() {
        // a got further before restarting.
        let round_end = round_end(vec![
            racer(
                "a",
                vec![race(&[10_000, 20_000], false), race(&[10_000], false)],
            ),
            dnf("b", &[11_000, 22_000]),
        ]);
        assert_eq!(
            order(&["a", "b"], &round_end, CountedRace::Best),
            ["b", "a"]
        );
    }

    #[test]
    fn equal_races_go_by_account() {
        // Names are only for display, a replay may not know them.
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
        let round_end = round_end(vec![
            finish("y", &race),
            finish("x", &race),
            finish("w", &race),
        ]);
        assert_eq!(
            order(&["y", "x", "w"], &round_end, CountedRace::Last),
            ["w", "x", "y"]
        );
        native::set(0, BTreeMap::new());
    }
}
