//! The races of the current round, built from the standard callbacks of the server.

use tm_server_types::event::WayPoint;

use crate::bindings::tmservers::driver::types::RaceResult;

/// A finished race: its time and the time at every checkpoint in milliseconds.
#[derive(Debug, Clone)]
struct Finish {
    time: u32,
    checkpoints: Vec<u32>,
}

#[derive(Debug, Default)]
struct Race {
    /// Checkpoints passed in the current race, the finish included.
    progress: u32,
    /// The race times at those checkpoints.
    progress_times: Vec<u32>,
    /// The last finished race of the round.
    previous: Option<Finish>,
    /// The best finished race of the round.
    best: Option<Finish>,
}

/// The players who started in the current round, in the order of their first start.
#[derive(Debug, Default)]
pub(crate) struct Races {
    players: Vec<(String, Race)>,
}

impl Races {
    pub(crate) fn clear(&mut self) {
        self.players.clear();
    }

    /// A player started a race. True for the first start of the player in the round.
    pub(crate) fn start(&mut self, account_id: &str) -> bool {
        match self.race_mut(account_id) {
            Some(race) => {
                race.progress = 0;
                race.progress_times.clear();
                false
            }
            None => {
                self.players.push((account_id.into(), Race::default()));
                true
            }
        }
    }

    /// A player passed a checkpoint or the finish. A player whose start the host
    /// missed still raced.
    pub(crate) fn way_point(&mut self, way_point: &WayPoint) {
        if self.race_mut(&way_point.account_id).is_none() {
            self.players
                .push((way_point.account_id.clone(), Race::default()));
        }
        let Some(race) = self.race_mut(&way_point.account_id) else {
            return;
        };

        // Counts from 0 for the first checkpoint.
        race.progress = way_point.checkpoint_in_race + 1;
        race.progress_times = way_point.current_race_checkpoints.clone();
        if way_point.is_end_race {
            let finish = Finish {
                time: way_point.racetime,
                checkpoints: way_point.current_race_checkpoints.clone(),
            };
            if race
                .best
                .as_ref()
                .is_none_or(|best| finish.time < best.time)
            {
                race.best = Some(finish.clone());
            }
            race.previous = Some(finish);
        }
    }

    /// Everyone who started in the round, then the connected players who did not.
    pub(crate) fn results<'a>(
        &self,
        connected: impl IntoIterator<Item = &'a String>,
    ) -> Vec<RaceResult> {
        let mut results: Vec<RaceResult> = self
            .players
            .iter()
            .map(|(account_id, race)| RaceResult {
                account_id: account_id.clone(),
                started: true,
                progress: race.progress,
                progress_times: race.progress_times.clone(),
                previous_time: race.previous.as_ref().map(|finish| finish.time),
                previous_checkpoints: checkpoints(&race.previous),
                best_time: race.best.as_ref().map(|finish| finish.time),
                best_checkpoints: checkpoints(&race.best),
            })
            .collect();

        for account_id in connected {
            if !self
                .players
                .iter()
                .any(|(started, _)| started == account_id)
            {
                results.push(RaceResult {
                    account_id: account_id.clone(),
                    started: false,
                    progress: 0,
                    progress_times: Vec::new(),
                    previous_time: None,
                    previous_checkpoints: Vec::new(),
                    best_time: None,
                    best_checkpoints: Vec::new(),
                });
            }
        }
        results
    }

    fn race_mut(&mut self, account_id: &str) -> Option<&mut Race> {
        self.players
            .iter_mut()
            .find(|(started, _)| started == account_id)
            .map(|(_, race)| race)
    }
}

fn checkpoints(finish: &Option<Finish>) -> Vec<u32> {
    finish
        .as_ref()
        .map_or_else(Vec::new, |finish| finish.checkpoints.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn way_point(account_id: &str, checkpoint_in_race: u32, finish: Option<u32>) -> WayPoint {
        WayPoint {
            account_id: account_id.into(),
            time: 0,
            racetime: finish.unwrap_or(0),
            laptime: 0,
            speed: 0.,
            checkpoint_in_race,
            checkpoint_in_lap: checkpoint_in_race,
            is_end_race: finish.is_some(),
            is_end_lap: finish.is_some(),
            is_infinite_laps: false,
            is_independent_laps: false,
            current_race_checkpoints: finish.into_iter().collect(),
            current_lap_checkpoints: vec![],
            block_id: String::new(),
        }
    }

    #[test]
    fn the_last_and_the_best_race_count() {
        let mut races = Races::default();
        assert!(races.start("a"));
        races.way_point(&way_point("a", 0, Some(31_000)));
        assert!(!races.start("a"));
        races.way_point(&way_point("a", 0, Some(30_000)));
        // Restarted and gave up at the start.
        races.start("a");

        let results = races.results([]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].progress, 0);
        assert!(results[0].progress_times.is_empty());
        assert_eq!(results[0].previous_time, Some(30_000));
        assert_eq!(results[0].best_time, Some(30_000));
        assert_eq!(results[0].best_checkpoints, [30_000]);
    }

    #[test]
    fn races_without_a_finish_keep_their_checkpoint_times() {
        let mut races = Races::default();
        races.start("a");
        races.way_point(&WayPoint {
            current_race_checkpoints: vec![10_000, 21_000],
            ..way_point("a", 1, None)
        });

        let results = races.results([]);
        assert_eq!(results[0].progress, 2);
        assert_eq!(results[0].progress_times, [10_000, 21_000]);
        assert_eq!(results[0].previous_time, None);
    }

    #[test]
    fn connected_players_without_a_start_take_no_part() {
        let mut races = Races::default();
        races.start("a");
        races.way_point(&way_point("a", 1, None));
        // The start was missed but the race still counts.
        races.way_point(&way_point("b", 0, None));

        let connected = ["a".to_string(), "s".to_string()];
        let results = races.results(&connected);
        let summary: Vec<(&str, bool, u32)> = results
            .iter()
            .map(|r| (r.account_id.as_str(), r.started, r.progress))
            .collect();
        assert_eq!(summary, [("a", true, 2), ("b", true, 1), ("s", false, 0)]);
    }
}
