//! The last players of a round lose a life, everyone without a finish as well, and
//! the last player left wins.

use tm_driver_mode_knockout::{ExtraLosses, Settings};
use tm_driver_sim::rules::{DriverStatus, RoundEnd, Rules, ends_match, entry, playing};

pub struct Knockout {
    settings: Settings,
}

impl Knockout {
    /// The rules for a match with these settings.
    pub fn boxed(settings: &str) -> Result<Box<dyn Rules>, String> {
        let settings: Settings = tm_driver_sim::rules::settings(settings)?;
        Ok(Box::new(Self { settings }))
    }

    /// How many players lose a life in a round of the match, before players without
    /// a finish add to them: one plus one for every milestone ahead, and at least
    /// one player stays.
    fn losses(&self, round: u32, players: usize, lives: usize) -> usize {
        if round <= self.settings.rounds_without_elimination {
            return 0;
        }
        let ahead = |milestones: &[u32], left: usize| {
            milestones
                .iter()
                .filter(|&&above| (above as usize) < left)
                .count()
        };
        let extra = match &self.settings.extra_losses {
            ExtraLosses::AbovePlayersLeft(milestones) => ahead(milestones, players),
            ExtraLosses::AboveLivesLeft(milestones) => ahead(milestones, lives),
            ExtraLosses::BeforeRound(milestones) => {
                milestones.iter().filter(|&&before| before > round).count()
            }
        };
        (1 + extra).min(players.saturating_sub(1))
    }

    /// The players who lose a life in a round: everyone in the match during it
    /// without a finish, filled up by the slowest finishers. If that would take the
    /// last life of everyone, the player who got the furthest keeps it.
    fn losers(&self, end: &RoundEnd) -> usize {
        // Everyone in the match during the round, with the lives they had before it.
        let in_round: Vec<(&str, i32)> = end
            .after
            .entries
            .iter()
            .filter_map(|after| {
                let before = entry(end.before, &after.account_id);
                let newly_out = after.status == DriverStatus::Eliminated
                    && before.is_none_or(|before| before.status != DriverStatus::Eliminated);
                (after.status == DriverStatus::Playing || newly_out).then(|| {
                    let lives = before
                        .filter(|before| before.status == DriverStatus::Playing && before.score > 0)
                        .map_or(self.settings.lives, |before| before.score);
                    (after.account_id.as_str(), lives)
                })
            })
            .collect();
        let lives = in_round
            .iter()
            .map(|&(_, lives)| lives.max(0) as usize)
            .sum();
        let without_finish = in_round
            .iter()
            .filter(|(account_id, _)| end.truth.last_finish(account_id).is_none())
            .count();
        let losses = match self.losses(end.round.number, in_round.len(), lives) {
            0 => 0,
            losses => losses.max(without_finish),
        };
        if losses > 0 && losses == in_round.len() && in_round.iter().all(|&(_, lives)| lives <= 1) {
            losses - 1
        } else {
            losses
        }
    }
}

impl Rules for Knockout {
    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        let eliminations = end.round.number > self.settings.rounds_without_elimination;

        // The players who lost a life are the last ones of the race.
        let mut lost = false;
        for entry in &end.round.entries {
            match entry.score {
                0 if lost => {
                    return Err(format!(
                        "{} keeps a life but finished behind someone who lost one",
                        entry.account_id
                    ));
                }
                0 => {}
                -1 => lost = true,
                score => {
                    return Err(format!(
                        "{} lost {} lives in a round",
                        entry.account_id, -score
                    ));
                }
            }
            // Everyone without a finish loses a life, unless that would leave nobody
            // and the player who got the furthest keeps theirs.
            if eliminations
                && entry.score == 0
                && end.truth.last_finish(&entry.account_id).is_none()
                && entry.position != 1
            {
                return Err(format!("{} kept a life without a finish", entry.account_id));
            }
        }
        if lost && !eliminations {
            return Err(format!(
                "Round {} took a life before the eliminations started",
                end.round.number
            ));
        }

        let mut in_round = 0;
        for after in &end.after.entries {
            let before = entry(end.before, &after.account_id);
            match after.status {
                DriverStatus::Playing => {
                    in_round += 1;
                    if after.score < 1 || after.score > self.settings.lives {
                        return Err(format!("{} has {} lives", after.account_id, after.score));
                    }
                    if let Some(before) = before
                        && before.status == DriverStatus::Playing
                        && after.score > before.score
                    {
                        return Err(format!(
                            "{} got a life: {} after {}",
                            after.account_id, after.score, before.score
                        ));
                    }
                }
                DriverStatus::Eliminated => {
                    if after.score != 0 {
                        return Err(format!(
                            "Eliminated {} keeps {} lives",
                            after.account_id, after.score
                        ));
                    }
                    if before.is_none_or(|before| before.status != DriverStatus::Eliminated) {
                        in_round += 1;
                        if before.is_some_and(|before| before.score > 1) {
                            return Err(format!(
                                "{} is eliminated with lives left",
                                after.account_id
                            ));
                        }
                        let placed = end
                            .round
                            .entries
                            .iter()
                            .find(|entry| entry.account_id == after.account_id);
                        if placed.is_none_or(|entry| entry.score != -1) {
                            return Err(format!(
                                "{} is eliminated without losing a life",
                                after.account_id
                            ));
                        }
                    }
                }
                DriverStatus::Spectator => {}
            }
        }
        if eliminations && in_round >= 2 && !lost {
            return Err(format!("Nobody of {in_round} players lost a life"));
        }
        let losers = end
            .round
            .entries
            .iter()
            .filter(|entry| entry.score == -1)
            .count();
        let expected = self.losers(end);
        if losers != expected {
            return Err(format!(
                "{losers} players lost a life in round {} instead of {expected}",
                end.round.number
            ));
        }

        let participants = end
            .after
            .entries
            .iter()
            .any(|entry| entry.status != DriverStatus::Spectator);
        let over = eliminations && participants && playing(end.after).count() <= 1;
        if ends_match(end.commands) {
            if !over {
                return Err("The match ends before one player is left".into());
            }
            if let Some(winner) = playing(end.after).find(|entry| entry.label != "Winner") {
                return Err(format!("{} wins without the label", winner.account_id));
            }
        } else if over {
            return Err("One player is left, but the match goes on".into());
        }
        Ok(())
    }
}
