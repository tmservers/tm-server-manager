//! Players lose points by their place in the race, run out of them into last
//! chance and are eliminated by losing the most in a round. The match ends once
//! the winners are left.

use std::collections::BTreeMap;

use tm_mode_driver_sim::rules::{
    Counted, DriverEntry, DriverStandings, DriverStatus, LiveRound, RoundEnd, Rules, ends_map,
    ends_match, entry, in_race_order, live_in_race_order, playing, points, racers_placed,
    spectators_stay,
};
use tm_mode_reverse_cup::{LastChanceDnfMode, Settings};

pub struct ReverseCup {
    settings: Settings,
    /// The complex repartition by every count of players left.
    complex: BTreeMap<usize, Vec<i32>>,
}

impl ReverseCup {
    /// The rules for a match with these settings.
    pub fn boxed(settings: &str) -> Result<Box<dyn Rules>, String> {
        Ok(Box::new(Self::new(settings)?))
    }

    fn new(settings: &str) -> Result<Self, String> {
        let settings: Settings = tm_mode_driver_sim::rules::settings(settings)?;
        let mut complex = BTreeMap::new();
        for (counts, repartition) in &settings.complex_points_repartition {
            for count in counts.split(',') {
                let count = count
                    .trim()
                    .parse()
                    .map_err(|_| format!("{counts} are no counts of players"))?;
                complex.insert(count, repartition.clone());
            }
        }
        Ok(Self { settings, complex })
    }

    /// The points lost by place with this many players left of the participants:
    /// the complex repartition for them or the normal one, which loses its first
    /// values to the eliminated players with fast forward.
    fn repartition(&self, left: usize, participants: usize) -> Vec<i32> {
        let mut repartition = self
            .complex
            .get(&left)
            .unwrap_or(&self.settings.points_repartition)
            .clone();
        if self.settings.fast_forward_points_repartition && left > repartition.len() {
            let eliminated = participants - left;
            repartition.drain(..eliminated.min(repartition.len()));
        }
        repartition
    }

    /// Everyone in the match during a round that counted loses points: finishers by
    /// their place, the others the dnf points. In last chance with
    /// `only_least_checkpoints`, only the racers without a finish who passed the
    /// fewest checkpoints lose them.
    fn check_losses(&self, end: &RoundEnd) -> Result<(), String> {
        let in_round: Vec<&DriverEntry> = end
            .after
            .entries
            .iter()
            .filter(|after| match after.status {
                DriverStatus::Playing => true,
                DriverStatus::Eliminated => entry(end.before, &after.account_id)
                    .is_none_or(|before| before.status != DriverStatus::Eliminated),
                DriverStatus::Spectator => false,
            })
            .collect();
        let repartition = self.repartition(in_round.len(), Self::participants(end.after));

        let in_last_chance = |account_id: &str| {
            entry(end.before, account_id)
                .is_some_and(|before| before.status == DriverStatus::Playing && before.score == 0)
        };
        let finished = |account_id: &str| end.truth.last_finish(account_id).is_some();
        let checkpoints = |account_id: &str| {
            end.truth
                .races(account_id)
                .and_then(|races| races.last())
                .map(|race| race.checkpoints.len())
        };
        let least_checkpoints = in_round
            .iter()
            .map(|entry| entry.account_id.as_str())
            .filter(|account_id| in_last_chance(account_id) && !finished(account_id))
            .filter_map(checkpoints)
            .min();

        let mut finishers = 0;
        for entry in &end.round.entries {
            let account_id = entry.account_id.as_str();
            let expected = if finished(account_id) {
                finishers += 1;
                -points(&repartition, finishers - 1)
            } else if self.settings.last_chance_dnf_mode == LastChanceDnfMode::OnlyLeastCheckpoints
                && in_last_chance(account_id)
                && checkpoints(account_id).is_some_and(|passed| Some(passed) != least_checkpoints)
            {
                0
            } else {
                -self.settings.dnf_points_loss
            };
            if entry.score != expected {
                return Err(format!(
                    "{account_id} lost {} points instead of {}",
                    -entry.score, -expected
                ));
            }
        }
        // Without a race, a player loses the dnf points and gets a place for that.
        for player in &in_round {
            if !end
                .round
                .entries
                .iter()
                .any(|entry| entry.account_id == player.account_id)
                && checkpoints(&player.account_id).is_none()
            {
                return Err(format!(
                    "{} did not race but lost no points",
                    player.account_id
                ));
            }
        }
        Ok(())
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
    fn settings_changed(&mut self, settings: &str) -> Result<(), String> {
        *self = Self::new(settings)?;
        Ok(())
    }

    /// The finishers so far lose the points of their place. Who loses the points
    /// for giving up is only known at the end.
    fn live_round(&mut self, live: &LiveRound) -> Result<(), String> {
        live_in_race_order(live, Counted::Last)?;
        let left = playing(live.standings).count();
        let repartition = self.repartition(left, Self::participants(live.standings));
        let mut finishers = 0;
        for entry in &live.round.entries {
            let expected = if live.truth.last_finish(&entry.account_id).is_some() {
                finishers += 1;
                -points(&repartition, finishers - 1)
            } else {
                0
            };
            if entry.score != expected {
                return Err(format!(
                    "{} would lose {} points instead of {}",
                    entry.account_id, -entry.score, -expected
                ));
            }
        }
        Ok(())
    }

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

        spectators_stay(end)?;
        let scored = !end.round.entries.is_empty();
        if scored {
            in_race_order(end, Counted::Last)?;
            racers_placed(end)?;
            self.check_losses(end)?;
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

        let limit = self.settings.rounds_per_map;
        let map_done = scored && limit > 0 && end.after.rounds_on_map >= limit;
        if ends_map(end.commands) != (map_done && !ends_match(end.commands)) {
            return Err(format!(
                "{} of {limit} rounds on the map, but the map ends: {}",
                end.after.rounds_on_map,
                ends_map(end.commands)
            ));
        }
        Ok(())
    }

    /// Every round that counts takes points from someone or eliminates a player in
    /// last chance. A match waiting for players may wait forever.
    fn round_limit(&self, standings: &DriverStandings) -> Option<u32> {
        let participants = Self::participants(standings) as u32;
        let points = self.settings.starting_points.max(0) as u32;
        (!self.waits(standings)).then(|| participants * (points + 1))
    }
}

fn entry_score(end: &RoundEnd, account_id: &str) -> i32 {
    end.round
        .entries
        .iter()
        .find(|entry| entry.account_id == account_id)
        .map_or(0, |entry| entry.score)
}
