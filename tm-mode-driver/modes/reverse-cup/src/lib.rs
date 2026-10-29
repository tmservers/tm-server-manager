//! Reverse cup mode for the wasm driver.
//!
//! Follows the rules of the community ReverseCup mode: every player starts with
//! points and loses points by their finish position. A player who runs out of
//! points is in last chance and is eliminated by losing the most points of a
//! round. The match ends once only the winners are left.
//!
//! The players who start a race before the first round counts take part in the
//! match with their starting points. Everyone else cannot spawn anymore. A match
//! needs more participants than winners: until then rounds do not count and the
//! match waits for players. A single participant plays alone until eliminated.
//!
//! The original mode skips a round that would put everyone in last chance
//! anyway. This mode does that right after the previous round, so the round is
//! not started at all.
//!
//! Participants, their points and statuses live in the standings the host keeps.
//! Players with points are playing, players in last chance have 0 points and
//! eliminated players keep the rank they were eliminated with. Nobody ties: equal
//! races go to the better times at the checkpoints and finally by account,
//! players with the same points by their race in the last round.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tm_mode_driver_sdk::{
    CanSpawn, Command, CountedRace, Entry, Event, Label, Mode, Racer, RoundEnd, RoundFormat,
    Standings, Status, Tone,
    settings::{FinishTimeoutSetting, WarmUpSetting},
};

tm_mode_driver_sdk::export_mode!(ReverseCup);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Points every participant starts with.
    pub starting_points: i32,
    /// Points lost by finish position. Finishers beyond the list lose the last value.
    pub points_repartition: Vec<i32>,
    /// Replaces `points_repartition` while this many players are left in the match.
    /// A key can list several counts, e.g. `{"3": [2, 6, 9], "4,5": [0, 4, 7, 10]}`.
    pub complex_points_repartition: BTreeMap<String, Vec<i32>>,
    /// While more players are left than the repartition has values, every
    /// eliminated player removes its first value, so the best finishers lose more.
    pub fast_forward_points_repartition: bool,
    /// Points lost without a finish.
    pub dnf_points_loss: i32,
    /// Players who run out of points are eliminated without a last chance.
    pub disable_last_chance: bool,
    pub last_chance_dnf_mode: LastChanceDnfMode,
    /// Put everyone in last chance once the next round would do that anyway.
    pub allow_fast_forward_rounds: bool,
    /// The match ends once this many players are left.
    pub number_of_winners: u32,
    /// Valid rounds played on every map. 0 means no limit.
    pub rounds_per_map: u32,
    pub finish_timeout: FinishTimeoutSetting,
    /// The warmup before the first round of every map.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            starting_points: 100,
            points_repartition: (1..=20).collect(),
            complex_points_repartition: BTreeMap::from([
                ("2".into(), vec![4, 8]),
                ("3".into(), vec![2, 6, 9]),
                ("4".into(), vec![0, 4, 7, 10]),
                ("5".into(), vec![0, 4, 7, 10, 13]),
            ]),
            fast_forward_points_repartition: true,
            dnf_points_loss: 20,
            disable_last_chance: false,
            last_chance_dnf_mode: LastChanceDnfMode::AllPlayers,
            allow_fast_forward_rounds: true,
            number_of_winners: 1,
            rounds_per_map: 5,
            finish_timeout: FinishTimeoutSetting::Automatic,
            warmup: WarmUpSetting::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LastChanceDnfMode {
    /// Every player in last chance without a finish loses the dnf points.
    AllPlayers,
    /// Only the players in last chance without a finish who passed the fewest
    /// checkpoints lose the dnf points. Players who did not start, for example
    /// because they left, always lose them.
    OnlyLeastCheckpoints,
}

#[derive(Debug, Clone)]
pub struct ReverseCup {
    settings: Settings,
    /// `complex_points_repartition` by every count of players left.
    complex_repartition: BTreeMap<usize, Vec<i32>>,
}

impl Mode for ReverseCup {
    const NAME: &'static str = "Reverse Cup";
    /// Everything but the points players start with, which participants have.
    const RELOADABLE: &'static [&'static str] = &[
        "points_repartition",
        "complex_points_repartition",
        "fast_forward_points_repartition",
        "dnf_points_loss",
        "disable_last_chance",
        "last_chance_dnf_mode",
        "allow_fast_forward_rounds",
        "number_of_winners",
        "rounds_per_map",
        "finish_timeout",
        "warmup",
    ];

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        let mut complex_repartition = BTreeMap::new();
        for (counts, repartition) in &settings.complex_points_repartition {
            for count in counts.split(',') {
                let count = count.trim().parse().map_err(|_| {
                    format!(
                        "Invalid settings: \"{counts}\" of complex_points_repartition is not a list of player counts"
                    )
                })?;
                complex_repartition.insert(count, repartition.clone());
            }
        }

        Ok(Self {
            settings,
            complex_repartition,
        })
    }

    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        let commands = self.handle(standings, event);
        self.label(standings);
        commands
    }

    /// The finishers so far with the points they would lose. Who loses the points
    /// for giving up and who is eliminated is only known once the round ended.
    fn live_round(&self, standings: &Standings, races: &RoundEnd) -> Vec<Entry> {
        // Like at the round end, nothing counts while the match waits or is over.
        if standings.participants().next().is_none()
            || self.waits(standings)
            || self.match_over(standings)
        {
            return Vec::new();
        }
        let finished = |account_id: &str| {
            races
                .racer(account_id)
                .is_some_and(|racer| CountedRace::Last.time(racer).is_some())
        };
        let finishers = tm_mode_driver_sdk::race_order(
            standings
                .in_match()
                .map(|p| p.account_id.as_str())
                .filter(|account_id| finished(account_id)),
            races,
            CountedRace::Last,
        );
        let repartition = self.repartition(standings);
        standings.placed(races, CountedRace::Last, |account_id| {
            finishers
                .iter()
                .position(|finisher| finisher == account_id)
                .map_or(0, |place| {
                    -repartition
                        .get(place)
                        .or(repartition.last())
                        .copied()
                        .unwrap_or(0)
                })
        })
    }
}

impl ReverseCup {
    fn handle(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active. New settings send them
            // again, the round format applies from the next round on.
            Event::MapStart | Event::SettingsChanged => {
                standings.info = self.info(standings);
                vec![
                    Command::SetRoundFormat(RoundFormat::SingleAttempt(
                        self.settings.finish_timeout.into(),
                    )),
                    Command::SetWarmUp(self.settings.warmup.into()),
                ]
            }
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            Event::MapEnd => {
                if self.match_over(standings) {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            // Players take part from their first race on, so the standings show them
            // with their starting points during the first round.
            Event::RaceStart(account_id) => {
                if self.registration_open(standings) && standings.player(&account_id).is_none() {
                    standings.join(&account_id).score = self.settings.starting_points;
                    standings.info = self.info(standings);
                }
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
    }

    /// Players in last chance and, once the match is over, the winners. The labels
    /// follow from the standings alone.
    fn label(&self, standings: &mut Standings) {
        let match_over = !self.registration_open(standings) && self.match_over(standings);
        for player in &mut standings.entries {
            player.label = if match_over && player.is_in_match() {
                Some(Label::winner())
            } else if in_last_chance(player) {
                Some(last_chance_label())
            } else {
                None
            };
        }
    }

    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // Starters the race starts did not bring in, e.g. after the driver restarted.
        if self.registration_open(standings) {
            self.register(standings, &round_end);
        }
        if round_end.invalid || standings.participants().next().is_none() || self.waits(standings) {
            standings.info = self.info(standings);
            return self.block_late_players(standings, &round_end);
        }
        // Over before the round, e.g. after more winners were set during the match.
        if self.match_over(standings) {
            standings.info = self.info(standings);
            let mut commands = self.block_late_players(standings, &round_end);
            commands.extend(winners_message(standings).map(Command::ChatMessage));
            commands.push(Command::EndMatch);
            return commands;
        }
        let mut commands = Vec::new();

        let losses = self.losses(standings, &round_end);
        let max_loss = losses
            .iter()
            .map(|&(_, loss)| loss)
            .min()
            .unwrap_or(0)
            .min(0);
        let left = standings.in_match().count();

        let mut eliminated = Vec::new();
        for (account_id, loss) in &losses {
            standings.round_entry(account_id).score = *loss;
            let Some(player) = standings.player_mut(account_id) else {
                continue;
            };
            if in_last_chance(player) {
                // Losing the most points of the round ends the last chance.
                if *loss < 0 && *loss == max_loss {
                    eliminated.push(account_id.clone());
                }
            } else if player.score.saturating_add(*loss) > 0 {
                player.score = player.score.saturating_add(*loss);
            } else if !self.settings.disable_last_chance {
                player.score = 0;
            } else {
                eliminated.push(account_id.clone());
            }
        }

        // Players eliminated in the same round take the last positions of the players
        // left, in the order of the race.
        let first_position = left - eliminated.len();
        for (index, account_id) in eliminated.into_iter().enumerate() {
            let position = (first_position + index + 1) as u32;
            let player = standings
                .player_mut(&account_id)
                .expect("Only participants lose points");
            player.status = Status::Eliminated;
            player.position = position;
            player.score = 0;
            commands.push(Command::ChatMessage(format!(
                "$f90$i$<{}$> takes {} place!",
                tm_mode_driver_sdk::name(&account_id),
                tm_mode_driver_sdk::ordinal(position)
            )));
            commands.push(Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn: false,
            }));
        }

        if losses.iter().any(|&(_, loss)| loss < 0) {
            standings.rounds_on_map += 1;
        }
        commands.extend(self.block_late_players(standings, &round_end));

        let match_over = self.match_over(standings);
        if !match_over && self.fast_forward(standings) {
            commands.push(Command::ChatMessage(
                "$c00Fast forward: every remaining player is now in Last Chance.".into(),
            ));
        }
        standings.rank(&round_end, CountedRace::Last);
        standings.place_round(&round_end, CountedRace::Last);
        standings.info = self.info(standings);

        if match_over {
            commands.extend(winners_message(standings).map(Command::ChatMessage));
            commands.push(Command::EndMatch);
        } else if self.settings.rounds_per_map > 0
            && standings.rounds_on_map >= self.settings.rounds_per_map
        {
            commands.push(Command::EndMap);
        }
        commands
    }

    /// Players can take part until the first round counts, which takes points from
    /// someone. A map ending before does not close it.
    fn registration_open(&self, standings: &Standings) -> bool {
        standings
            .participants()
            .all(|p| p.is_in_match() && p.score == self.settings.starting_points)
    }

    /// Too few players take part to have a loser, so the match waits for more.
    fn waits(&self, standings: &Standings) -> bool {
        let participants = standings.participants().count();
        self.registration_open(standings)
            && participants >= 2
            && participants <= self.settings.number_of_winners as usize
    }

    /// Everyone who started a race takes part while registration is open.
    fn register(&self, standings: &mut Standings, round_end: &RoundEnd) {
        for racer in &round_end.racers {
            if standings.player(&racer.account_id).is_none() {
                standings.join(&racer.account_id).score = self.settings.starting_points;
            }
        }
    }

    /// Players who could still take part are not blocked.
    fn block_late_players(&self, standings: &mut Standings, round_end: &RoundEnd) -> Vec<Command> {
        if self.registration_open(standings) {
            Vec::new()
        } else {
            standings.block_others(round_end)
        }
    }

    /// Points lost by every player still in the match in the order of the race:
    /// finishers by their finish position, everyone without a finish the dnf points.
    fn losses(&self, standings: &Standings, round_end: &RoundEnd) -> Vec<(String, i32)> {
        let order = tm_mode_driver_sdk::race_order(
            standings.in_match().map(|p| p.account_id.as_str()),
            round_end,
            CountedRace::Last,
        );
        let finished = |racer: &Racer| CountedRace::Last.time(racer).is_some();
        // How far a player without a finish got.
        let checkpoints =
            |racer: &Racer| racer.races.last().map_or(0, |race| race.checkpoints.len());
        let last_chance =
            |account_id: &str| standings.player(account_id).is_some_and(in_last_chance);
        let least_checkpoints = order
            .iter()
            .filter(|account_id| last_chance(account_id))
            .filter_map(|account_id| round_end.racer(account_id))
            .filter(|racer| !finished(racer))
            .map(checkpoints)
            .min();

        let repartition = self.repartition(standings);
        let mut finishers = 0;
        order
            .into_iter()
            .map(|account_id| {
                // Players who did not start, for example because they left, have no race.
                let loss = match round_end.racer(&account_id) {
                    Some(racer) if finished(racer) => {
                        let loss = repartition
                            .get(finishers)
                            .or(repartition.last())
                            .copied()
                            .unwrap_or(0);
                        finishers += 1;
                        loss
                    }
                    racer => match (self.settings.last_chance_dnf_mode, racer) {
                        (LastChanceDnfMode::OnlyLeastCheckpoints, Some(racer))
                            if last_chance(&account_id)
                                && Some(checkpoints(racer)) != least_checkpoints =>
                        {
                            0
                        }
                        _ => self.settings.dnf_points_loss,
                    },
                };
                (account_id, -loss)
            })
            .collect()
    }

    /// Points lost by finish position with the players left in the match.
    fn repartition(&self, standings: &Standings) -> Vec<i32> {
        let left = standings.in_match().count();
        let mut repartition = self
            .complex_repartition
            .get(&left)
            .unwrap_or(&self.settings.points_repartition)
            .clone();
        if self.settings.fast_forward_points_repartition && left > repartition.len() {
            let eliminated = standings.participants().count() - left;
            repartition.drain(..eliminated.min(repartition.len()));
        }
        repartition
    }

    /// Puts everyone in last chance when the next round would do that anyway:
    /// nobody is in last chance yet and the first place loses all points left.
    fn fast_forward(&self, standings: &mut Standings) -> bool {
        if !self.settings.allow_fast_forward_rounds {
            return false;
        }
        let Some(&first) = self.repartition(standings).first() else {
            return false;
        };
        let pointless = standings.entries.iter().all(|p| match p.status {
            // Nobody may be in last chance yet.
            Status::Playing => p.score > 0 && p.score <= first,
            Status::Eliminated | Status::Spectator => true,
        });
        if !pointless {
            return false;
        }
        for player in standings.entries.iter_mut().filter(|p| p.is_in_match()) {
            player.score = 0;
        }
        true
    }

    /// Only the winners are left. A single participant plays until eliminated, a
    /// match waiting for players has not even started.
    fn match_over(&self, standings: &Standings) -> bool {
        let left = standings.in_match().count();
        match standings.participants().count() {
            0 => false,
            1 => left == 0,
            _ if self.waits(standings) => false,
            _ => left <= self.settings.number_of_winners as usize,
        }
    }

    fn info(&self, standings: &Standings) -> String {
        if self.waits(standings) {
            return format!(
                "Waiting for players, {} / {}",
                standings.participants().count(),
                self.settings.number_of_winners + 1
            );
        }
        let rounds = match self.settings.rounds_per_map {
            0 => format!("{} rounds", standings.rounds_on_map),
            limit => format!("{} / {limit} rounds", standings.rounds_on_map),
        };
        if standings.participants().next().is_none() {
            rounds
        } else {
            format!("{} left, {rounds}", standings.in_match().count())
        }
    }
}

/// Players in last chance have no points left but are still in the match.
fn in_last_chance(player: &Entry) -> bool {
    player.is_in_match() && player.score == 0
}

fn last_chance_label() -> Label {
    Label::new("Last Chance", Tone::Negative)
}

/// Everyone left in the match wins once it is over.
fn winners_message(standings: &Standings) -> Option<String> {
    let winners: Vec<String> = standings
        .in_match()
        .map(|p| format!("$<{}$>", tm_mode_driver_sdk::name(&p.account_id)))
        .collect();
    match winners.as_slice() {
        [] => None,
        [winner] => Some(format!("{winner} wins the match!")),
        _ => Some(format!("{} win the match!", winners.join(", "))),
    }
}

#[cfg(test)]
mod tests;
