//! The races of the current round as they happened, from the standard callbacks of
//! the server.

use tm_server_types::event::WayPoint;

use crate::bindings::tmservers::driver::types::{Race, Racer};

/// The players who started in the current round with their races, in the order of
/// their first start.
#[derive(Debug, Default)]
pub(crate) struct Races {
    racers: Vec<Racer>,
}

impl Races {
    pub(crate) fn clear(&mut self) {
        self.racers.clear();
    }

    /// A player started a race. True for the first start of the player in the round.
    pub(crate) fn start(&mut self, account_id: &str) -> bool {
        let first = !self.racers.iter().any(|r| r.account_id == account_id);
        self.races_mut(account_id).push(new_race());
        first
    }

    /// A player passed a checkpoint or the finish. A race whose start the host
    /// missed still counts.
    pub(crate) fn way_point(&mut self, way_point: &WayPoint) {
        let races = self.races_mut(&way_point.account_id);
        // Only a new start comes after the finish.
        if races.last().is_none_or(|race| race.finished) {
            races.push(new_race());
        }
        let race = races.last_mut().expect("The player has a race");
        // Every checkpoint of the race so far, the finish included.
        race.checkpoints
            .clone_from(&way_point.current_race_checkpoints);
        race.finished = way_point.is_end_race;
        // Counted from the checkpoints rather than by lap, so a missed waypoint loses
        // no lap.
        if way_point.is_end_lap {
            race.laps = (way_point.checkpoint_in_race + 1) / (way_point.checkpoint_in_lap + 1);
        }
    }

    /// Everyone who started in the round with their races.
    pub(crate) fn racers(&self) -> Vec<Racer> {
        self.racers.clone()
    }

    /// The races of a player, who becomes a racer of the round.
    fn races_mut(&mut self, account_id: &str) -> &mut Vec<Race> {
        let index = match self.racers.iter().position(|r| r.account_id == account_id) {
            Some(index) => index,
            None => {
                self.racers.push(Racer {
                    account_id: account_id.into(),
                    races: Vec::new(),
                });
                self.racers.len() - 1
            }
        };
        &mut self.racers[index].races
    }
}

fn new_race() -> Race {
    Race {
        checkpoints: Vec::new(),
        finished: false,
        laps: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player passed a checkpoint, counting from 0, ten seconds after the one before
    /// or at the finish time.
    fn way_point(account_id: &str, checkpoint_in_race: u32, finish: Option<u32>) -> WayPoint {
        let mut checkpoints: Vec<u32> = (1..=checkpoint_in_race + 1)
            .map(|checkpoint| checkpoint * 10_000)
            .collect();
        if let Some(finish) = finish {
            *checkpoints.last_mut().unwrap() = finish;
        }
        WayPoint {
            account_id: account_id.into(),
            time: 0,
            racetime: checkpoints[checkpoints.len() - 1],
            laptime: 0,
            speed: 0.,
            checkpoint_in_race,
            checkpoint_in_lap: checkpoint_in_race,
            is_end_race: finish.is_some(),
            is_end_lap: finish.is_some(),
            is_infinite_laps: false,
            is_independent_laps: false,
            current_race_checkpoints: checkpoints,
            current_lap_checkpoints: vec![],
            block_id: String::new(),
        }
    }

    /// Every race by racer: the account, the checkpoint times and whether it finished.
    fn summary(races: &Races) -> Vec<(String, Vec<u32>, bool)> {
        races
            .racers()
            .into_iter()
            .flat_map(|racer| {
                racer
                    .races
                    .into_iter()
                    .map(move |race| (racer.account_id.clone(), race.checkpoints, race.finished))
            })
            .collect()
    }

    #[test]
    fn every_race_is_kept() {
        let mut races = Races::default();
        assert!(races.start("a"));
        races.way_point(&way_point("a", 0, Some(31_000)));
        assert!(!races.start("a"));
        races.way_point(&way_point("a", 0, Some(30_000)));
        // Restarted and gave up at the start.
        races.start("a");

        assert_eq!(
            summary(&races),
            [
                ("a".to_string(), vec![31_000], true),
                ("a".to_string(), vec![30_000], true),
                ("a".to_string(), vec![], false),
            ]
        );
    }

    #[test]
    fn races_without_a_finish_keep_their_checkpoint_times() {
        let mut races = Races::default();
        races.start("a");
        races.way_point(&WayPoint {
            current_race_checkpoints: vec![10_000, 21_000],
            ..way_point("a", 1, None)
        });

        assert_eq!(
            summary(&races),
            [("a".to_string(), vec![10_000, 21_000], false)]
        );
    }

    #[test]
    fn laps_count_once_they_end() {
        // Two checkpoints and the line that ends a lap.
        let lap_point = |checkpoint_in_race: u32| WayPoint {
            checkpoint_in_lap: checkpoint_in_race % 3,
            is_end_lap: checkpoint_in_race % 3 == 2,
            ..way_point("a", checkpoint_in_race, None)
        };
        let laps = |races: &Races| races.racers()[0].races[0].laps;
        let mut races = Races::default();
        races.start("a");
        races.way_point(&lap_point(1));
        assert_eq!(laps(&races), 0);
        races.way_point(&lap_point(2));
        assert_eq!(laps(&races), 1);
        races.way_point(&lap_point(4));
        assert_eq!(laps(&races), 1);
        // The end of the second lap got lost, the third one still counts all three.
        races.way_point(&lap_point(8));
        assert_eq!(laps(&races), 3);
    }

    #[test]
    fn races_whose_start_was_missed_still_count() {
        let mut races = Races::default();
        races.start("a");
        races.way_point(&way_point("a", 0, Some(30_000)));
        // The starts of the next race of a and of the race of b were missed.
        races.way_point(&way_point("b", 1, None));
        races.way_point(&way_point("a", 0, None));

        // In the order of the first starts.
        assert_eq!(
            summary(&races),
            [
                ("a".to_string(), vec![30_000], true),
                ("a".to_string(), vec![10_000], false),
                ("b".to_string(), vec![10_000, 20_000], false),
            ]
        );
        // b started already.
        assert!(!races.start("b"));
    }
}
