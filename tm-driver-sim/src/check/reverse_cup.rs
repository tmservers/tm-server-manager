//! Players only lose points, run out of them into last chance and are eliminated
//! by losing the most in a round. The match ends once the winners are left.

use tm_driver_mode_reverse_cup::Settings;
use tm_server_types::method::{DriverStandings, DriverStatus};

use super::{ModeChecks, RoundEnd, ends_match, entry, playing};

pub(crate) struct ReverseCup {
    settings: Settings,
}

impl ReverseCup {
    pub(crate) fn new(settings: Settings) -> Self {
        Self { settings }
    }

    /// Only the winners are left once registration closed. A single participant
    /// plays until eliminated. While registration is open, rounds in which nobody
    /// loses points do not count, so a match with no more participants than winners
    /// waits for more players.
    fn over(&self, standings: &DriverStandings) -> bool {
        if standings.rounds_on_map == 0 && standings.maps_played == 0 {
            return false;
        }
        let participants = standings
            .entries
            .iter()
            .filter(|entry| entry.status != DriverStatus::Spectator)
            .count();
        let left = playing(standings).count();
        match participants {
            0 => false,
            1 => left == 0,
            _ => left <= self.settings.number_of_winners as usize,
        }
    }
}

impl ModeChecks for ReverseCup {
    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
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
        if over && !end.ended && !ends_match(end.commands) {
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
