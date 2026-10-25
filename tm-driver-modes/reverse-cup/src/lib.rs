//! Reverse cup mode for the wasm driver.
//!
//! Follows the rules of the community ReverseCup mode: every player starts with
//! points and loses points by their finish position. A player who runs out of
//! points is in last chance and is eliminated by losing the most points of a
//! round. The match ends once only the winners are left.
//!
//! The players who start in the first valid round take part in the match.
//! Everyone else cannot spawn anymore. The original mode skips a round that
//! would put everyone in last chance anyway. This mode does that right after
//! the previous round, so the round is not started at all.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
};

use serde::Deserialize;
use tm_driver_sdk::{
    CanSpawn, Command, Event, Mode, PlayerResult, RaceResult, RoundEnd, RoundFormat,
    settings::FinishTimeoutSetting,
    standings::{Standing, Standings},
};

tm_driver_sdk::export_mode!(ReverseCup);

#[derive(Debug, Clone, Deserialize)]
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LastChanceDnfMode {
    /// Every player in last chance without a finish loses the dnf points.
    AllPlayers,
    /// Only the players in last chance without a finish who passed the fewest
    /// checkpoints lose the dnf points. Players who did not start, for example
    /// because they left, always lose them.
    OnlyLeastCheckpoints,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Alive(i32),
    LastChance,
    /// With the final rank of the player.
    Eliminated(u32),
}

#[derive(Debug, Clone)]
struct Participant {
    name: String,
    status: Status,
    /// Points lost in the last round.
    round_points: i32,
}

impl Participant {
    fn in_match(&self) -> bool {
        !matches!(self.status, Status::Eliminated(_))
    }
}

#[derive(Debug, Clone)]
pub struct ReverseCup {
    settings: Settings,
    /// `complex_points_repartition` by every count of players left.
    complex_repartition: BTreeMap<usize, Vec<i32>>,
    participants: BTreeMap<String, Participant>,
    /// Players who cannot spawn because they do not take part.
    blocked: BTreeSet<String>,
    valid_rounds_on_map: u32,
    match_over: bool,
}

impl Mode for ReverseCup {
    fn init(settings: &str) -> Result<Self, String> {
        let settings: Settings = if settings.trim().is_empty() {
            Settings::default()
        } else {
            serde_json::from_str(settings).map_err(|e| format!("Invalid settings: {e}"))?
        };

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
            participants: BTreeMap::new(),
            blocked: BTreeSet::new(),
            valid_rounds_on_map: 0,
            match_over: false,
        })
    }

    fn step(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::MatchStart => {
                self.participants.clear();
                self.blocked.clear();
                self.match_over = false;
                vec![self.standings()]
            }
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active.
            Event::MapStart => {
                self.valid_rounds_on_map = 0;
                vec![
                    Command::SetRoundFormat(RoundFormat::SingleAttempt(
                        self.settings.finish_timeout.into(),
                    )),
                    self.standings(),
                ]
            }
            Event::RoundStart => Vec::new(),
            Event::RoundEnd(round_end) => self.round_end(round_end),
            Event::MapEnd => {
                if self.match_over {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            Event::MatchEnd => Vec::new(),
        }
    }
}

impl ReverseCup {
    fn round_end(&mut self, round_end: RoundEnd) -> Vec<Command> {
        // Forced rounds do not count, not even to find the participants.
        if self.participants.is_empty() && !round_end.forced {
            self.register(&round_end.results);
        }
        let mut commands = self.block_others(&round_end.results);
        if round_end.forced || self.participants.is_empty() || self.match_over {
            return commands;
        }

        for result in &round_end.results {
            // Players who left before the scores were sent have no name.
            if let Some(participant) = self.participants.get_mut(&result.account_id)
                && !result.name.is_empty()
            {
                participant.name = result.name.clone();
            }
        }

        let losses = self.losses(&round_end.results);
        let max_loss = losses
            .iter()
            .map(|&(_, loss)| loss)
            .min()
            .unwrap_or(0)
            .min(0);
        let eliminated_before = self.participants.len() - self.left();

        for participant in self.participants.values_mut() {
            participant.round_points = 0;
        }
        let mut eliminated = Vec::new();
        for (account_id, loss) in &losses {
            let Some(participant) = self.participants.get_mut(account_id) else {
                continue;
            };
            participant.round_points = *loss;
            match participant.status {
                Status::Alive(points) if points.saturating_add(*loss) > 0 => {
                    participant.status = Status::Alive(points.saturating_add(*loss));
                }
                Status::Alive(_) if !self.settings.disable_last_chance => {
                    participant.status = Status::LastChance;
                }
                Status::Alive(_) => eliminated.push(account_id.clone()),
                // Losing the most points of the round ends the last chance.
                Status::LastChance if *loss < 0 && *loss == max_loss => {
                    eliminated.push(account_id.clone());
                }
                Status::LastChance | Status::Eliminated(_) => {}
            }
        }

        // Players eliminated in the same round share their rank.
        let rank = (self.participants.len() - eliminated_before - eliminated.len() + 1) as u32;
        for account_id in eliminated {
            let participant = self
                .participants
                .get_mut(&account_id)
                .expect("Only participants lose points");
            participant.status = Status::Eliminated(rank);
            commands.push(Command::ChatMessage(format!(
                "$f90$i$<{}$> takes {} place!",
                participant.name,
                ordinal(rank)
            )));
            commands.push(Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn: false,
            }));
        }

        if losses.iter().any(|&(_, loss)| loss < 0) {
            self.valid_rounds_on_map += 1;
        }

        let left = self.left();
        self.match_over = if self.participants.len() == 1 {
            left == 0
        } else {
            left <= self.settings.number_of_winners as usize
        };
        if self.match_over {
            commands.extend(self.winners_message().map(Command::ChatMessage));
        } else if self.fast_forward() {
            commands.push(Command::ChatMessage(
                "$c00Fast forward: every remaining player is now in Last Chance.".into(),
            ));
        }

        commands.push(Command::RoundResult(
            losses
                .into_iter()
                .enumerate()
                .map(|(index, (account_id, points))| PlayerResult {
                    account_id,
                    points,
                    position: index as u32 + 1,
                })
                .collect(),
        ));
        commands.push(self.standings());
        if self.match_over {
            commands.push(Command::EndMatch);
        } else if self.settings.rounds_per_map > 0
            && self.valid_rounds_on_map >= self.settings.rounds_per_map
        {
            commands.push(Command::EndMap);
        }
        commands
    }

    /// The players who started in the first valid round take part in the match.
    fn register(&mut self, results: &[RaceResult]) {
        for result in results.iter().filter(|r| r.started) {
            let name = if result.name.is_empty() {
                &result.account_id
            } else {
                &result.name
            };
            self.participants.insert(
                result.account_id.clone(),
                Participant {
                    name: name.clone(),
                    status: Status::Alive(self.settings.starting_points),
                    round_points: 0,
                },
            );
        }
    }

    /// Everyone else cannot spawn anymore once the participants are known.
    fn block_others(&mut self, results: &[RaceResult]) -> Vec<Command> {
        let mut commands = Vec::new();
        if self.participants.is_empty() {
            return commands;
        }
        for result in results {
            if !self.participants.contains_key(&result.account_id)
                && self.blocked.insert(result.account_id.clone())
            {
                commands.push(Command::SetCanSpawn(CanSpawn {
                    account_id: result.account_id.clone(),
                    can_spawn: false,
                }));
            }
        }
        commands
    }

    /// Points lost by every player still in the match in the order of the round:
    /// finishers by time, then everyone without a finish by checkpoints passed.
    fn losses(&self, results: &[RaceResult]) -> Vec<(String, i32)> {
        let mut finishers: Vec<&RaceResult> = results
            .iter()
            .filter(|r| {
                r.previous_time.is_some()
                    && self
                        .participants
                        .get(&r.account_id)
                        .is_some_and(Participant::in_match)
            })
            .collect();
        // Stable so equal times keep the order of the scores.
        finishers.sort_by_key(|r| r.previous_time);

        let repartition = self.repartition();
        let mut losses: Vec<(String, i32)> = finishers
            .iter()
            .enumerate()
            .map(|(rank, finisher)| {
                let loss = repartition
                    .get(rank)
                    .or(repartition.last())
                    .copied()
                    .unwrap_or(0);
                (finisher.account_id.clone(), -loss)
            })
            .collect();

        // Players who did not start, for example because they left, have no progress.
        let finished: BTreeSet<&str> = finishers.iter().map(|r| r.account_id.as_str()).collect();
        let progress: BTreeMap<&str, u32> = results
            .iter()
            .filter(|r| r.started)
            .map(|r| (r.account_id.as_str(), r.progress))
            .collect();
        let mut dnfs: Vec<(&String, Status, Option<u32>)> = self
            .participants
            .iter()
            .filter(|(account_id, participant)| {
                participant.in_match() && !finished.contains(account_id.as_str())
            })
            .map(|(account_id, participant)| {
                (
                    account_id,
                    participant.status,
                    progress.get(account_id.as_str()).copied(),
                )
            })
            .collect();
        // Stable so equal progress keeps a fixed order.
        dnfs.sort_by_key(|&(_, _, progress)| Reverse(progress));

        let least_checkpoints = dnfs
            .iter()
            .filter(|&&(_, status, _)| status == Status::LastChance)
            .filter_map(|&(_, _, progress)| progress)
            .min();
        for (account_id, status, progress) in dnfs {
            let loss = match (self.settings.last_chance_dnf_mode, status, progress) {
                (LastChanceDnfMode::OnlyLeastCheckpoints, Status::LastChance, Some(progress))
                    if Some(progress) != least_checkpoints =>
                {
                    0
                }
                _ => -self.settings.dnf_points_loss,
            };
            losses.push((account_id.clone(), loss));
        }
        losses
    }

    /// Points lost by finish position with the players left in the match.
    fn repartition(&self) -> Vec<i32> {
        let left = self.left();
        let mut repartition = self
            .complex_repartition
            .get(&left)
            .unwrap_or(&self.settings.points_repartition)
            .clone();
        if self.settings.fast_forward_points_repartition && left > repartition.len() {
            let eliminated = self.participants.len() - left;
            repartition.drain(..eliminated.min(repartition.len()));
        }
        repartition
    }

    /// Puts everyone in last chance when the next round would do that anyway:
    /// nobody is in last chance yet and the first place loses all points left.
    fn fast_forward(&mut self) -> bool {
        if !self.settings.allow_fast_forward_rounds {
            return false;
        }
        let Some(&first) = self.repartition().first() else {
            return false;
        };
        let pointless = self.participants.values().all(|p| match p.status {
            Status::Alive(points) => points <= first,
            Status::LastChance => false,
            Status::Eliminated(_) => true,
        });
        if !pointless {
            return false;
        }
        for participant in self.participants.values_mut() {
            if let Status::Alive(_) = participant.status {
                participant.status = Status::LastChance;
            }
        }
        true
    }

    /// Players still in the match.
    fn left(&self) -> usize {
        self.participants.values().filter(|p| p.in_match()).count()
    }

    fn winners_message(&self) -> Option<String> {
        let winners: Vec<String> = self
            .ranked()
            .into_iter()
            .filter(|(_, participant)| participant.in_match())
            .map(|(_, participant)| format!("$<{}$>", participant.name))
            .collect();
        match winners.as_slice() {
            [] => None,
            [winner] => Some(format!("{winner} wins the match!")),
            _ => Some(format!("{} win the match!", winners.join(", "))),
        }
    }

    fn standings(&self) -> Command {
        let players = self
            .ranked()
            .into_iter()
            .map(|(account_id, participant)| {
                let (points, status) = match participant.status {
                    Status::Alive(points) => (points, ""),
                    Status::LastChance => (0, "Last Chance"),
                    Status::Eliminated(_) => (0, "Eliminated"),
                };
                Standing {
                    account_id: account_id.clone(),
                    name: participant.name.clone(),
                    points,
                    round_points: participant.round_points,
                    status: status.into(),
                }
            })
            .collect();

        let rounds = match self.settings.rounds_per_map {
            0 => format!("{} rounds", self.valid_rounds_on_map),
            limit => format!("{} / {limit} rounds", self.valid_rounds_on_map),
        };
        Standings {
            title: "Reverse Cup".into(),
            info: if self.participants.is_empty() {
                rounds
            } else {
                format!("{} left, {rounds}", self.left())
            },
            players,
        }
        .command()
    }

    /// Participants in the order of the standings: players with points by their
    /// points, then the last chance, then the eliminated players by rank.
    fn ranked(&self) -> Vec<(&String, &Participant)> {
        let mut ranked: Vec<_> = self.participants.iter().collect();
        // Stable so ties keep a fixed order.
        ranked.sort_by_key(|(_, participant)| match participant.status {
            Status::Alive(points) => (0, -i64::from(points)),
            Status::LastChance => (1, 0),
            Status::Eliminated(rank) => (2, i64::from(rank)),
        });
        ranked
    }
}

/// 1st, 2nd, 3rd, 4th, …, 11th, 12th, 13th, …, 21st, …
fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use tm_driver_sdk::FinishTimeout;

    use super::*;

    fn finish(account_id: &str, time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
            started: true,
            progress: 3,
            previous_time: Some(time),
            previous_checkpoints: vec![time / 2, time / 2, time],
            best_time: Some(time),
            best_checkpoints: vec![time / 2, time / 2, time],
        }
    }

    /// Gave up after passing a number of checkpoints.
    fn dnf(account_id: &str, progress: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
            started: true,
            progress,
            previous_time: None,
            previous_checkpoints: vec![],
            best_time: None,
            best_checkpoints: vec![],
        }
    }

    /// Did not start in the round.
    fn spectator(account_id: &str) -> RaceResult {
        RaceResult {
            started: false,
            ..dnf(account_id, 0)
        }
    }

    fn round_end(results: Vec<RaceResult>) -> Event {
        Event::RoundEnd(RoundEnd {
            forced: false,
            results,
        })
    }

    fn started(settings: &str) -> ReverseCup {
        let mut mode = ReverseCup::init(settings).unwrap();
        mode.step(Event::MatchStart);
        mode.step(Event::MapStart);
        mode
    }

    /// A match whose participants already reached these statuses.
    fn playing(settings: &str, players: &[(&str, Status)]) -> ReverseCup {
        let mut mode = started(settings);
        for &(account_id, status) in players {
            mode.participants.insert(
                account_id.into(),
                Participant {
                    name: account_id.into(),
                    status,
                    round_points: 0,
                },
            );
        }
        mode
    }

    fn status(mode: &ReverseCup, account_id: &str) -> Status {
        mode.participants[account_id].status
    }

    fn round_result(commands: &[Command]) -> Vec<(String, i32, u32)> {
        commands
            .iter()
            .find_map(|c| match c {
                Command::RoundResult(results) => Some(
                    results
                        .iter()
                        .map(|r| (r.account_id.clone(), r.points, r.position))
                        .collect(),
                ),
                _ => None,
            })
            .expect("Every counted round has a result")
    }

    fn chat(commands: &[Command]) -> Vec<&str> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::ChatMessage(message) => Some(message.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Players who cannot spawn anymore.
    fn blocked(commands: &[Command]) -> Vec<&str> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::SetCanSpawn(CanSpawn {
                    account_id,
                    can_spawn: false,
                }) => Some(account_id.as_str()),
                _ => None,
            })
            .collect()
    }

    fn standings(commands: &[Command]) -> serde_json::Value {
        commands
            .iter()
            .find_map(|c| match c {
                Command::SetNetState(state) => Some(serde_json::from_str(state).unwrap()),
                _ => None,
            })
            .expect("The standings are published")
    }

    #[test]
    fn sets_the_round_format_on_map_start() {
        let mut mode = ReverseCup::init(r#"{"finish_timeout": {"milliseconds": 7000}}"#).unwrap();
        assert_eq!(
            mode.step(Event::MapStart)[0],
            Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                7000
            )))
        );
    }

    #[test]
    fn players_who_start_the_first_round_take_part() {
        let mut mode = started("");
        let commands = mode.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 2),
            spectator("s"),
        ]));
        assert_eq!(mode.participants.keys().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(blocked(&commands), ["s"]);
        // The first round counts already. Two players lose 4 and 8 points.
        assert_eq!(
            round_result(&commands),
            [("a".into(), -4, 1), ("b".into(), -20, 2)]
        );
        assert_eq!(status(&mode, "a"), Status::Alive(96));
        assert_eq!(status(&mode, "b"), Status::Alive(80));

        // Late players cannot spawn anymore once they show up and their races do not count.
        let commands = mode.step(round_end(vec![
            finish("c", 29_000),
            finish("a", 30_000),
            finish("b", 31_000),
            spectator("s"),
        ]));
        assert_eq!(blocked(&commands), ["c"]);
        assert_eq!(
            round_result(&commands),
            [("a".into(), -4, 1), ("b".into(), -8, 2)]
        );
    }

    #[test]
    fn forced_rounds_do_not_count() {
        let mut mode = started("");
        let forced = |results| {
            Event::RoundEnd(RoundEnd {
                forced: true,
                results,
            })
        };
        assert!(mode.step(forced(vec![finish("a", 30_000)])).is_empty());
        assert!(mode.participants.is_empty());

        let mut mode = playing("", &[("a", Status::Alive(50))]);
        let commands = mode.step(forced(vec![finish("a", 30_000), finish("c", 31_000)]));
        assert_eq!(blocked(&commands), ["c"]);
        assert_eq!(status(&mode, "a"), Status::Alive(50));
        assert_eq!(mode.valid_rounds_on_map, 0);
    }

    #[test]
    fn points_lost_by_finish_order() {
        let mut mode = playing(
            r#"{"points_repartition": [1, 2], "complex_points_repartition": {}}"#,
            &[
                ("a", Status::Alive(50)),
                ("b", Status::Alive(50)),
                ("c", Status::Alive(50)),
                ("d", Status::Alive(50)),
            ],
        );
        let commands = mode.step(round_end(vec![
            finish("b", 31_000),
            finish("a", 30_000),
            dnf("d", 1),
            finish("c", 32_000),
        ]));
        assert_eq!(
            round_result(&commands),
            [
                ("a".into(), -1, 1),
                ("b".into(), -2, 2),
                // Beyond the repartition everyone loses the last value.
                ("c".into(), -2, 3),
                ("d".into(), -20, 4),
            ]
        );
        assert_eq!(status(&mode, "d"), Status::Alive(30));
    }

    #[test]
    fn repartition_by_players_left() {
        let alive = [
            ("a", Status::Alive(50)),
            ("b", Status::Alive(50)),
            ("c", Status::Alive(50)),
        ];
        let mode = playing(
            r#"{"complex_points_repartition": {"2, 3": [5, 10]}}"#,
            &alive,
        );
        assert_eq!(mode.repartition(), [5, 10]);

        let players = [
            ("a", Status::Alive(50)),
            ("b", Status::Alive(50)),
            ("c", Status::Alive(50)),
            ("d", Status::Alive(50)),
            ("e", Status::Eliminated(5)),
        ];
        let mut mode = playing(
            r#"{"points_repartition": [1, 2, 3], "complex_points_repartition": {}}"#,
            &players,
        );
        // More players left than values: the eliminated player removes the first value.
        assert_eq!(mode.repartition(), [2, 3]);
        mode.settings.fast_forward_points_repartition = false;
        assert_eq!(mode.repartition(), [1, 2, 3]);

        let mode = playing(
            r#"{"points_repartition": [1, 2, 3], "complex_points_repartition": {}}"#,
            &players[1..],
        );
        assert_eq!(mode.repartition(), [1, 2, 3]);
    }

    #[test]
    fn last_chance_ends_by_losing_the_most() {
        let mut mode = playing(
            r#"{"points_repartition": [5, 10], "complex_points_repartition": {}}"#,
            &[
                ("a", Status::Alive(20)),
                ("b", Status::Alive(8)),
                ("c", Status::LastChance),
            ],
        );
        let commands = mode.step(round_end(vec![
            finish("a", 30_000),
            finish("b", 31_000),
            finish("c", 32_000),
        ]));
        assert_eq!(status(&mode, "a"), Status::Alive(15));
        // Players with points get a last chance, even if they lost the most.
        assert_eq!(status(&mode, "b"), Status::LastChance);
        assert_eq!(status(&mode, "c"), Status::Eliminated(3));
        assert_eq!(blocked(&commands), ["c"]);
        assert_eq!(chat(&commands), ["$f90$i$<c$> takes 3rd place!"]);
        assert!(!commands.contains(&Command::EndMatch));
    }

    #[test]
    fn last_chance_survives_when_someone_else_lost_more() {
        let mut mode = playing(
            "",
            &[
                ("a", Status::Alive(50)),
                ("b", Status::LastChance),
                ("c", Status::LastChance),
            ],
        );
        let commands = mode.step(round_end(vec![
            finish("b", 30_000),
            finish("c", 31_000),
            dnf("a", 2),
        ]));
        assert_eq!(
            round_result(&commands),
            [
                ("b".into(), -2, 1),
                ("c".into(), -6, 2),
                ("a".into(), -20, 3)
            ]
        );
        assert_eq!(status(&mode, "b"), Status::LastChance);
        assert_eq!(status(&mode, "c"), Status::LastChance);
        assert!(chat(&commands).is_empty());
    }

    #[test]
    fn only_the_least_checkpoints_lose_in_last_chance() {
        let mut mode = playing(
            r#"{"last_chance_dnf_mode": "only_least_checkpoints"}"#,
            &[
                ("a", Status::LastChance),
                ("b", Status::LastChance),
                ("c", Status::LastChance),
                ("d", Status::LastChance),
                ("e", Status::Alive(50)),
            ],
        );
        let commands = mode.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 3),
            dnf("c", 1),
            dnf("e", 2),
            spectator("d"),
        ]));
        assert_eq!(
            round_result(&commands),
            [
                // Five players lose 0, 4, 7, 10 and 13 points.
                ("a".into(), 0, 1),
                ("b".into(), 0, 2),
                ("e".into(), -20, 3),
                ("c".into(), -20, 4),
                // Did not start, for example because they left.
                ("d".into(), -20, 5),
            ]
        );
        assert_eq!(status(&mode, "a"), Status::LastChance);
        assert_eq!(status(&mode, "b"), Status::LastChance);
        assert_eq!(status(&mode, "c"), Status::Eliminated(4));
        assert_eq!(status(&mode, "d"), Status::Eliminated(4));
        assert_eq!(status(&mode, "e"), Status::Alive(30));
    }

    #[test]
    fn every_dnf_loses_in_last_chance_by_default() {
        let mut mode = playing(
            "",
            &[
                ("a", Status::LastChance),
                ("b", Status::LastChance),
                ("c", Status::LastChance),
            ],
        );
        mode.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 3),
            dnf("c", 1),
        ]));
        assert_eq!(status(&mode, "a"), Status::LastChance);
        assert_eq!(status(&mode, "b"), Status::Eliminated(2));
        assert_eq!(status(&mode, "c"), Status::Eliminated(2));
    }

    #[test]
    fn eliminates_without_last_chance() {
        let mut mode = playing(
            r#"{"disable_last_chance": true, "points_repartition": [5, 10], "complex_points_repartition": {}}"#,
            &[
                ("a", Status::Alive(20)),
                ("b", Status::Alive(8)),
                ("c", Status::Alive(30)),
            ],
        );
        mode.step(round_end(vec![
            finish("a", 30_000),
            finish("b", 31_000),
            finish("c", 32_000),
        ]));
        assert_eq!(status(&mode, "a"), Status::Alive(15));
        assert_eq!(status(&mode, "b"), Status::Eliminated(3));
        assert_eq!(status(&mode, "c"), Status::Alive(20));
    }

    #[test]
    fn match_ends_once_the_winners_are_left() {
        let mut mode = playing(
            "",
            &[
                ("a", Status::Alive(50)),
                ("b", Status::LastChance),
                ("c", Status::LastChance),
            ],
        );
        let commands = mode.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 2),
            dnf("c", 2),
        ]));
        assert_eq!(status(&mode, "b"), Status::Eliminated(2));
        assert_eq!(status(&mode, "c"), Status::Eliminated(2));
        assert_eq!(
            chat(&commands),
            [
                "$f90$i$<b$> takes 2nd place!",
                "$f90$i$<c$> takes 2nd place!",
                "$<a$> wins the match!",
            ]
        );
        assert_eq!(commands.last(), Some(&Command::EndMatch));
        assert_eq!(mode.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn match_of_one_player_ends_with_the_elimination() {
        // One player loses 1 point as the first place.
        let mut mode = playing("", &[("a", Status::Alive(3))]);
        let commands = mode.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(status(&mode, "a"), Status::Alive(2));
        assert!(!commands.contains(&Command::EndMatch));

        mode.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(status(&mode, "a"), Status::LastChance);
        let commands = mode.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(status(&mode, "a"), Status::Eliminated(1));
        assert_eq!(chat(&commands), ["$f90$i$<a$> takes 1st place!"]);
        assert_eq!(commands.last(), Some(&Command::EndMatch));
    }

    #[test]
    fn fast_forwards_when_nobody_can_keep_points() {
        let players = [
            ("a", Status::Alive(6)),
            ("b", Status::Alive(12)),
            ("c", Status::Eliminated(3)),
        ];
        let mut mode = playing("", &players);
        let commands = mode.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        // a has 2 and b 4 points left. The next round costs the first place 4 points.
        assert_eq!(status(&mode, "a"), Status::LastChance);
        assert_eq!(status(&mode, "b"), Status::LastChance);
        assert_eq!(
            chat(&commands),
            ["$c00Fast forward: every remaining player is now in Last Chance."]
        );

        let mut mode = playing(r#"{"allow_fast_forward_rounds": false}"#, &players);
        mode.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        assert_eq!(status(&mode, "a"), Status::Alive(2));
        assert_eq!(status(&mode, "b"), Status::Alive(4));
    }

    #[test]
    fn map_ends_after_rounds_with_lost_points() {
        let mut mode = playing(
            r#"{"rounds_per_map": 1, "complex_points_repartition": {"2": [0, 0]}}"#,
            &[("a", Status::Alive(50)), ("b", Status::Alive(50))],
        );
        // Nobody lost points, so the round does not count.
        let commands = mode.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        assert!(!commands.contains(&Command::EndMap));

        let commands = mode.step(round_end(vec![finish("a", 30_000), dnf("b", 0)]));
        assert_eq!(commands.last(), Some(&Command::EndMap));
        // Next map, the match goes on.
        assert!(mode.step(Event::MapEnd).is_empty());
    }

    #[test]
    fn publishes_standings() {
        let mut mode = playing(
            r#"{"rounds_per_map": 5}"#,
            &[
                ("a", Status::Alive(50)),
                ("b", Status::LastChance),
                ("c", Status::Eliminated(5)),
                ("d", Status::Alive(60)),
                ("e", Status::Alive(5)),
            ],
        );
        // Four players lose 0, 4, 7 and 10 points.
        let commands = mode.step(round_end(vec![
            finish("d", 30_000),
            finish("a", 31_000),
            finish("e", 32_000),
            finish("b", 33_000),
        ]));

        assert_eq!(
            standings(&commands),
            serde_json::json!({
                "title": "Reverse Cup",
                "info": "3 left, 1 / 5 rounds",
                "players": [
                    {"account_id": "d", "name": "d", "points": 60, "round_points": 0, "status": ""},
                    {"account_id": "a", "name": "a", "points": 46, "round_points": -4, "status": ""},
                    {"account_id": "e", "name": "e", "points": 0, "round_points": -7, "status": "Last Chance"},
                    {"account_id": "b", "name": "b", "points": 0, "round_points": -10, "status": "Eliminated"},
                    {"account_id": "c", "name": "c", "points": 0, "round_points": 0, "status": "Eliminated"},
                ]
            })
        );
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(ReverseCup::init(r#"{"starting_point": 100}"#).is_err());
        assert!(ReverseCup::init(r#"{"complex_points_repartition": {"two": [4, 8]}}"#).is_err());
        assert!(ReverseCup::init(r#"{"last_chance_dnf_mode": "only_least_checkpoints"}"#).is_ok());
    }

    #[test]
    fn ordinals() {
        let ordinals: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            ordinals,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
    }
}
