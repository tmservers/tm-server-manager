//! The race is one round: players rank by the checkpoints they collected over all
//! their races, equal ones by who got there first, and the race ends the match.

use std::cmp::Reverse;

use tm_driver_mode_endurance::Settings;
use tm_driver_sim::rules::{
    DriverRound, DriverStandings, LiveRound, RoundEnd, RoundTruth, Rules, ends_match, entry,
    racers_placed,
};

pub struct Endurance {
    settings: Settings,
}

impl Endurance {
    /// The rules for a match with these settings.
    pub fn boxed(settings: &str) -> Result<Box<dyn Rules>, String> {
        let settings: Settings = tm_driver_sim::rules::settings(settings)?;
        Ok(Box::new(Self { settings }))
    }

    /// The round is ordered by the progress of every player and shows it: the
    /// checkpoints as points, the race times at the last checkpoints added up and
    /// the lap.
    fn check_places(&self, round: &DriverRound, truth: &RoundTruth) -> Result<(), String> {
        for pair in round.entries.windows(2) {
            let (a, b) = (&pair[0].account_id, &pair[1].account_id);
            if progress(truth, a).key(a) > progress(truth, b).key(b) {
                return Err(format!("{a} is placed before {b}, who got further"));
            }
        }
        for entry in &round.entries {
            let progress = progress(truth, &entry.account_id);
            let time = if progress.checkpoints > 0 {
                progress.time as i32
            } else {
                -1
            };
            if (entry.score, entry.time) != (progress.checkpoints as i32, time) {
                return Err(format!(
                    "{} has {} checkpoints at {}, not {} at {time}",
                    entry.account_id, entry.score, entry.time, progress.checkpoints
                ));
            }
            let lap = progress.laps + 1;
            let label = match self.settings.laps {
                Some(_) if progress.finished => "Finished".to_string(),
                Some(laps) => format!("Lap {} / {laps}", lap.min(laps)),
                None => format!("Lap {lap}"),
            };
            if entry.label != label {
                return Err(format!(
                    "{} is labeled {}, not {label}",
                    entry.account_id, entry.label
                ));
            }
        }
        Ok(())
    }
}

impl Rules for Endurance {
    fn settings_changed(&mut self, settings: &str) -> Result<(), String> {
        self.settings = tm_driver_sim::rules::settings(settings)?;
        Ok(())
    }

    fn live_round(&mut self, live: &LiveRound) -> Result<(), String> {
        self.check_places(live.round, live.truth)
    }

    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        racers_placed(end)?;
        self.check_places(end.round, end.truth)?;
        if !ends_match(end.commands) {
            return Err("The race does not end the match".into());
        }
        // The race is the result of the match.
        for place in &end.round.entries {
            let player = entry(end.after, &place.account_id)
                .ok_or(format!("{} is not in the standings", place.account_id))?;
            if (player.position, player.score) != (place.position, place.score) {
                return Err(format!(
                    "{} ranks {} with {} in the match, but placed {} with {} in the race",
                    place.account_id, player.position, player.score, place.position, place.score
                ));
            }
        }
        Ok(())
    }

    /// The first race that counts ends the match.
    fn round_limit(&self, _standings: &DriverStandings) -> Option<u32> {
        Some(1)
    }
}

/// How far a player got over all their races of the round.
struct Progress {
    checkpoints: u32,
    time: u32,
    laps: u32,
    finished: bool,
}

impl Progress {
    /// What orders the players, the best first.
    fn key<'a>(&self, account_id: &'a str) -> (Reverse<u32>, u32, &'a str) {
        (Reverse(self.checkpoints), self.time, account_id)
    }
}

fn progress(truth: &RoundTruth, account_id: &str) -> Progress {
    let races = truth.races(account_id).unwrap_or_default();
    Progress {
        checkpoints: races.iter().map(|race| race.checkpoints.len() as u32).sum(),
        time: races
            .iter()
            .filter_map(|race| race.checkpoints.last())
            .fold(0, |sum: u32, &time| sum.saturating_add(time)),
        laps: races.iter().map(|race| race.laps).sum(),
        finished: races.iter().any(|race| race.finished),
    }
}
