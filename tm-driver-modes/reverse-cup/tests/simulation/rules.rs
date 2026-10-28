//! Players only lose points, run out of them into last chance and are eliminated
//! by losing the most in a round. The match ends once the winners are left.

use tm_driver_mode_reverse_cup::Settings;
use tm_driver_sim::rules::{
    DriverStandings, DriverStatus, RoundEnd, Rules, ends_match, entry, playing,
};

pub struct ReverseCup {
    settings: Settings,
}

impl ReverseCup {
    /// The rules for a match with these settings.
    pub fn boxed(settings: &str) -> Result<Box<dyn Rules>, String> {
        let settings: Settings = tm_driver_sim::rules::settings(settings)?;
        Ok(Box::new(Self { settings }))
    }

    fn participants(standings: &DriverStandings) -> usize {
        standings
            .entries
            .iter()
            .filter(|entry| entry.status != DriverStatus::Spectator)
            .count()
    }

    /// Players can join until the first round counts, which takes points from
    /// someone.
    fn registration_open(&self, standings: &DriverStandings) -> bool {
        standings
            .entries
            .iter()
            .filter(|entry| entry.status != DriverStatus::Spectator)
            .all(|entry| {
                entry.status == DriverStatus::Playing
                    && entry.score == self.settings.starting_points
            })
    }

    /// No more participants than winners: rounds do not count until more players
    /// take part. A single participant plays alone.
    fn waits(&self, standings: &DriverStandings) -> bool {
        let participants = Self::participants(standings);
        self.registration_open(standings)
            && participants >= 2
            && participants <= self.settings.number_of_winners as usize
    }

    /// Only the winners are left once a round counted. A single participant plays
    /// until eliminated.
    fn over(&self, standings: &DriverStandings) -> bool {
        if self.registration_open(standings) {
            return false;
        }
        let left = playing(standings).count();
        match Self::participants(standings) {
            0 => false,
            1 => left == 0,
            _ => left <= self.settings.number_of_winners as usize,
        }
    }
}

impl Rules for ReverseCup {
    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        // Registration was still open when the round ended, and with the players who
        // registered in it there are still no more participants than winners.
        let participants = Self::participants(end.after);
        let waited = self.registration_open(end.before)
            && participants >= 2
            && participants <= self.settings.number_of_winners as usize;
        if waited && !end.round.entries.is_empty() {
            return Err("A round counted while the match waits for players".into());
        }
        let waiting = self.waits(end.after);
        if waiting != end.after.info.starts_with("Waiting for players") {
            return Err(format!(
                "The match waits for players: {waiting}, but the info is {}",
                end.after.info
            ));
        }

        let most = end.round.entries.iter().map(|entry| entry.score).min();
        if let Some(gain) = end.round.entries.iter().find(|entry| entry.score > 0) {
            return Err(format!("{} gained points in a round", gain.account_id));
        }

        for after in &end.after.entries {
            let before = entry(end.before, &after.account_id);
            match after.status {
                DriverStatus::Playing => {
                    if after.score < 0 || after.score > self.settings.starting_points {
                        return Err(format!("{} has {} points", after.account_id, after.score));
                    }
                    if after.score == 0 && after.label != "Last Chance" && after.label != "Winner" {
                        return Err(format!(
                            "{} has no points left but is not in last chance",
                            after.account_id
                        ));
                    }
                    if let Some(before) = before
                        && before.status == DriverStatus::Playing
                        && after.score > before.score
                    {
                        return Err(format!(
                            "{} got points: {} after {}",
                            after.account_id, after.score, before.score
                        ));
                    }
                }
                DriverStatus::Eliminated => {
                    if after.score != 0 {
                        return Err(format!(
                            "Eliminated {} keeps {} points",
                            after.account_id, after.score
                        ));
                    }
                    let newly =
                        before.is_none_or(|before| before.status != DriverStatus::Eliminated);
                    if newly {
                        let lost = entry_score(end, &after.account_id);
                        if lost >= 0 {
                            return Err(format!(
                                "{} is eliminated without losing points",
                                after.account_id
                            ));
                        }
                        if !self.settings.disable_last_chance && Some(lost) != most {
                            return Err(format!(
                                "{} is eliminated losing {lost} while someone lost {most:?}",
                                after.account_id
                            ));
                        }
                    }
                }
                DriverStatus::Spectator => {}
            }
        }

        let over = self.over(end.after);
        if ends_match(end.commands) && !over {
            return Err("The match ends before only the winners are left".into());
        }
        if over && !ends_match(end.commands) {
            return Err(format!(
                "Only the winners are left, but the match goes on: {} of {} players left, {} winners",
                playing(end.after).count(),
                end.after
                    .entries
                    .iter()
                    .filter(|entry| entry.status != DriverStatus::Spectator)
                    .count(),
                self.settings.number_of_winners
            ));
        }
        Ok(())
    }
}

fn entry_score(end: &RoundEnd, account_id: &str) -> i32 {
    end.round
        .entries
        .iter()
        .find(|entry| entry.account_id == account_id)
        .map_or(0, |entry| entry.score)
}
