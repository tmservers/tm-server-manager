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
//!
//! Participants, their points and statuses live in the standings the host keeps.
//! Players with points are playing, players in last chance have 0 points and
//! eliminated players keep the rank they were eliminated with.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet},
};

use serde::Deserialize;
use tm_driver_sdk::{
    CanSpawn, Command, Event, Mode, RaceResult, RoundEnd, RoundFormat, Standings, Status,
    settings::FinishTimeoutSetting,
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

#[derive(Debug, Clone)]
pub struct ReverseCup {
    settings: Settings,
    /// `complex_points_repartition` by every count of players left.
    complex_repartition: BTreeMap<usize, Vec<i32>>,
}

impl Mode for ReverseCup {
    const NAME: &'static str = "Reverse Cup";

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
        })
    }

    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active.
            Event::MapStart => {
                standings.info = self.info(standings);
                vec![Command::SetRoundFormat(RoundFormat::SingleAttempt(
                    self.settings.finish_timeout.into(),
                ))]
            }
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            Event::MapEnd => {
                if self.match_over(standings) {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
    }
}

impl ReverseCup {
    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // Forced rounds do not count, not even to find the participants.
        if standings.players.is_empty() && !round_end.forced {
            self.register(standings, &round_end.results);
        }
        let mut commands = block_others(standings, &round_end.results);
        if round_end.forced || standings.players.is_empty() || self.match_over(standings) {
            return commands;
        }

        for result in &round_end.results {
            // Players who left before the scores were sent have no name.
            if let Some(player) = standings.player_mut(&result.account_id)
                && !result.name.is_empty()
            {
                player.name = result.name.clone();
            }
        }

        let losses = self.losses(standings, &round_end.results);
        let max_loss = losses
            .iter()
            .map(|&(_, loss)| loss)
            .min()
            .unwrap_or(0)
            .min(0);
        let participants = standings.players.len();
        let eliminated_before = participants - standings.in_match().count();

        let mut eliminated = Vec::new();
        for (account_id, loss) in &losses {
            let Some(player) = standings.player_mut(account_id) else {
                continue;
            };
            player.round_points = *loss;
            match player.status {
                Status::Playing if player.points.saturating_add(*loss) > 0 => {
                    player.points = player.points.saturating_add(*loss);
                }
                Status::Playing if !self.settings.disable_last_chance => {
                    player.status = Status::LastChance;
                    player.points = 0;
                }
                Status::Playing => eliminated.push(account_id.clone()),
                // Losing the most points of the round ends the last chance.
                Status::LastChance if *loss < 0 && *loss == max_loss => {
                    eliminated.push(account_id.clone());
                }
                Status::LastChance | Status::Finalist | Status::Eliminated | Status::Winner => {}
            }
        }

        // Players eliminated in the same round share their rank.
        let rank = (participants - eliminated_before - eliminated.len() + 1) as u32;
        for account_id in eliminated {
            let player = standings
                .player_mut(&account_id)
                .expect("Only participants lose points");
            player.status = Status::Eliminated;
            player.rank = rank;
            player.points = 0;
            commands.push(Command::ChatMessage(format!(
                "$f90$i$<{}$> takes {} place!",
                player.name,
                ordinal(rank)
            )));
            commands.push(Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn: false,
            }));
        }

        if losses.iter().any(|&(_, loss)| loss < 0) {
            standings.rounds_on_map += 1;
        }

        let match_over = self.match_over(standings);
        if match_over {
            // Everyone left wins.
            for player in standings
                .players
                .iter_mut()
                .filter(|p| p.status != Status::Eliminated)
            {
                player.status = Status::Winner;
            }
        } else if self.fast_forward(standings) {
            commands.push(Command::ChatMessage(
                "$c00Fast forward: every remaining player is now in Last Chance.".into(),
            ));
        }
        standings.rank();
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

    /// The players who started in the first valid round take part in the match.
    fn register(&self, standings: &mut Standings, results: &[RaceResult]) {
        for result in results.iter().filter(|r| r.started) {
            standings.join(&result.account_id, &result.name).points = self.settings.starting_points;
        }
    }

    /// Points lost by every player still in the match: finishers by time, then
    /// everyone without a finish by checkpoints passed.
    fn losses(&self, standings: &Standings, results: &[RaceResult]) -> Vec<(String, i32)> {
        let in_match = |account_id: &str| {
            standings
                .player(account_id)
                .is_some_and(|p| p.status != Status::Eliminated)
        };
        let mut finishers: Vec<&RaceResult> = results
            .iter()
            .filter(|r| r.previous_time.is_some() && in_match(&r.account_id))
            .collect();
        // Stable so equal times keep the order of the scores.
        finishers.sort_by_key(|r| r.previous_time);

        let repartition = self.repartition(standings);
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
        let mut dnfs: Vec<(&str, Status, Option<u32>)> = standings
            .in_match()
            .filter(|p| !finished.contains(p.account_id.as_str()))
            .map(|p| {
                let account_id = p.account_id.as_str();
                (account_id, p.status, progress.get(account_id).copied())
            })
            .collect();
        dnfs.sort_by_key(|&(account_id, _, progress)| (Reverse(progress), account_id));

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
            losses.push((account_id.into(), loss));
        }
        losses
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
            let eliminated = standings.players.len() - left;
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
        let pointless = standings.players.iter().all(|p| match p.status {
            Status::Playing => p.points <= first,
            Status::Eliminated => true,
            Status::LastChance | Status::Finalist | Status::Winner => false,
        });
        if !pointless {
            return false;
        }
        for player in standings
            .players
            .iter_mut()
            .filter(|p| p.status == Status::Playing)
        {
            player.status = Status::LastChance;
            player.points = 0;
        }
        true
    }

    /// Only the winners are left. A single participant plays until eliminated.
    fn match_over(&self, standings: &Standings) -> bool {
        let left = standings.in_match().count();
        match standings.players.len() {
            0 => false,
            1 => left == 0,
            _ => left <= self.settings.number_of_winners as usize,
        }
    }

    fn info(&self, standings: &Standings) -> String {
        let rounds = match self.settings.rounds_per_map {
            0 => format!("{} rounds", standings.rounds_on_map),
            limit => format!("{} / {limit} rounds", standings.rounds_on_map),
        };
        if standings.players.is_empty() {
            rounds
        } else {
            format!("{} left, {rounds}", standings.in_match().count())
        }
    }
}

/// Everyone else cannot spawn anymore once the participants are known.
/// The host leaves out the players it blocked before.
fn block_others(standings: &Standings, results: &[RaceResult]) -> Vec<Command> {
    if standings.players.is_empty() {
        return Vec::new();
    }
    results
        .iter()
        .filter(|r| standings.player(&r.account_id).is_none())
        .map(|r| {
            Command::SetCanSpawn(CanSpawn {
                account_id: r.account_id.clone(),
                can_spawn: false,
            })
        })
        .collect()
}

fn winners_message(standings: &Standings) -> Option<String> {
    let winners: Vec<String> = standings
        .players
        .iter()
        .filter(|p| p.status == Status::Winner)
        .map(|p| format!("$<{}$>", p.name))
        .collect();
    match winners.as_slice() {
        [] => None,
        [winner] => Some(format!("{winner} wins the match!")),
        _ => Some(format!("{} win the match!", winners.join(", "))),
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
    use tm_driver_sdk::{FinishTimeout, Standing, testing::Harness};

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

    fn standing(account_id: &str, status: Status, points: i32, rank: u32) -> Standing {
        Standing {
            account_id: account_id.into(),
            name: account_id.into(),
            rank,
            status,
            points,
            map_points: 0,
            round_points: 0,
        }
    }

    fn alive(account_id: &str, points: i32) -> Standing {
        standing(account_id, Status::Playing, points, 0)
    }

    fn last_chance(account_id: &str) -> Standing {
        standing(account_id, Status::LastChance, 0, 0)
    }

    fn eliminated(account_id: &str, rank: u32) -> Standing {
        standing(account_id, Status::Eliminated, 0, rank)
    }

    fn started(settings: &str) -> Harness<ReverseCup> {
        let mut harness = Harness::new(settings).unwrap();
        harness.step(Event::MatchStart);
        harness.step(Event::MapStart);
        harness
    }

    /// A match whose participants already reached these standings.
    fn playing(settings: &str, players: Vec<Standing>) -> Harness<ReverseCup> {
        let mut harness = started(settings);
        harness.standings.players = players;
        harness
    }

    fn status(harness: &Harness<ReverseCup>, account_id: &str) -> (Status, i32) {
        let player = harness.standing(account_id);
        (player.status, player.points)
    }

    fn round_points<'a>(
        harness: &'a Harness<ReverseCup>,
        accounts: &[&'a str],
    ) -> Vec<(&'a str, i32)> {
        accounts
            .iter()
            .map(|&account_id| (account_id, harness.standing(account_id).round_points))
            .collect()
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

    #[test]
    fn sets_the_round_format_on_map_start() {
        let mut harness =
            Harness::<ReverseCup>::new(r#"{"finish_timeout": {"milliseconds": 7000}}"#).unwrap();
        assert_eq!(
            harness.step(Event::MapStart),
            [Command::SetRoundFormat(RoundFormat::SingleAttempt(
                FinishTimeout::Milliseconds(7000)
            ))]
        );
    }

    #[test]
    fn players_who_start_the_first_round_take_part() {
        let mut harness = started("");
        let commands = harness.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 2),
            spectator("s"),
        ]));
        let participants: Vec<&str> = harness
            .standings
            .players
            .iter()
            .map(|p| p.account_id.as_str())
            .collect();
        assert_eq!(participants, ["a", "b"]);
        assert_eq!(blocked(&commands), ["s"]);
        // The first round counts already. Two players lose 4 and 8 points.
        assert_eq!(round_points(&harness, &["a", "b"]), [("a", -4), ("b", -20)]);
        assert_eq!(status(&harness, "a"), (Status::Playing, 96));
        assert_eq!(status(&harness, "b"), (Status::Playing, 80));

        // Late players cannot spawn anymore once they show up and their races do
        // not count. The host leaves out s, who got blocked before.
        let commands = harness.step(round_end(vec![
            finish("c", 29_000),
            finish("a", 30_000),
            finish("b", 31_000),
            spectator("s"),
        ]));
        assert_eq!(blocked(&commands), ["c", "s"]);
        assert_eq!(round_points(&harness, &["a", "b"]), [("a", -4), ("b", -8)]);
        assert!(harness.standings.player("c").is_none());
    }

    #[test]
    fn forced_rounds_do_not_count() {
        let forced = |results| {
            Event::RoundEnd(RoundEnd {
                forced: true,
                results,
            })
        };
        let mut harness = started("");
        assert!(harness.step(forced(vec![finish("a", 30_000)])).is_empty());
        assert!(harness.standings.players.is_empty());

        let mut harness = playing("", vec![alive("a", 50)]);
        let commands = harness.step(forced(vec![finish("a", 30_000), finish("c", 31_000)]));
        assert_eq!(blocked(&commands), ["c"]);
        assert_eq!(status(&harness, "a"), (Status::Playing, 50));
        assert_eq!(harness.standings.rounds_on_map, 0);
    }

    #[test]
    fn points_lost_by_finish_order() {
        let mut harness = playing(
            r#"{"points_repartition": [1, 2], "complex_points_repartition": {}}"#,
            vec![
                alive("a", 50),
                alive("b", 50),
                alive("c", 50),
                alive("d", 50),
            ],
        );
        harness.step(round_end(vec![
            finish("b", 31_000),
            finish("a", 30_000),
            dnf("d", 1),
            finish("c", 32_000),
        ]));
        assert_eq!(
            round_points(&harness, &["a", "b", "c", "d"]),
            [
                ("a", -1),
                ("b", -2),
                // Beyond the repartition everyone loses the last value.
                ("c", -2),
                ("d", -20),
            ]
        );
        assert_eq!(status(&harness, "d"), (Status::Playing, 30));
    }

    #[test]
    fn repartition_by_players_left() {
        let mode =
            ReverseCup::init(r#"{"complex_points_repartition": {"2, 3": [5, 10]}}"#).unwrap();
        let mut standings = Standings {
            players: vec![alive("a", 50), alive("b", 50), alive("c", 50)],
            ..Default::default()
        };
        assert_eq!(mode.repartition(&standings), [5, 10]);

        let mut mode = ReverseCup::init(
            r#"{"points_repartition": [1, 2, 3], "complex_points_repartition": {}}"#,
        )
        .unwrap();
        standings.players = vec![
            alive("a", 50),
            alive("b", 50),
            alive("c", 50),
            alive("d", 50),
            eliminated("e", 5),
        ];
        // More players left than values: the eliminated player removes the first value.
        assert_eq!(mode.repartition(&standings), [2, 3]);
        mode.settings.fast_forward_points_repartition = false;
        assert_eq!(mode.repartition(&standings), [1, 2, 3]);

        mode.settings.fast_forward_points_repartition = true;
        standings.players.remove(0);
        assert_eq!(mode.repartition(&standings), [1, 2, 3]);
    }

    #[test]
    fn last_chance_ends_by_losing_the_most() {
        let mut harness = playing(
            r#"{"points_repartition": [5, 10], "complex_points_repartition": {}}"#,
            vec![alive("a", 20), alive("b", 8), last_chance("c")],
        );
        let commands = harness.step(round_end(vec![
            finish("a", 30_000),
            finish("b", 31_000),
            finish("c", 32_000),
        ]));
        assert_eq!(status(&harness, "a"), (Status::Playing, 15));
        // Players with points get a last chance, even if they lost the most.
        assert_eq!(status(&harness, "b"), (Status::LastChance, 0));
        assert_eq!(status(&harness, "c"), (Status::Eliminated, 0));
        assert_eq!(harness.standing("c").rank, 3);
        assert_eq!(blocked(&commands), ["c"]);
        assert_eq!(chat(&commands), ["$f90$i$<c$> takes 3rd place!"]);
        assert!(!commands.contains(&Command::EndMatch));
    }

    #[test]
    fn last_chance_survives_when_someone_else_lost_more() {
        let mut harness = playing("", vec![alive("a", 50), last_chance("b"), last_chance("c")]);
        let commands = harness.step(round_end(vec![
            finish("b", 30_000),
            finish("c", 31_000),
            dnf("a", 2),
        ]));
        assert_eq!(
            round_points(&harness, &["a", "b", "c"]),
            [("a", -20), ("b", -2), ("c", -6)]
        );
        assert_eq!(status(&harness, "b"), (Status::LastChance, 0));
        assert_eq!(status(&harness, "c"), (Status::LastChance, 0));
        assert!(chat(&commands).is_empty());
    }

    #[test]
    fn only_the_least_checkpoints_lose_in_last_chance() {
        let mut harness = playing(
            r#"{"last_chance_dnf_mode": "only_least_checkpoints"}"#,
            vec![
                last_chance("a"),
                last_chance("b"),
                last_chance("c"),
                last_chance("d"),
                alive("e", 50),
            ],
        );
        harness.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 3),
            dnf("c", 1),
            dnf("e", 2),
            spectator("d"),
        ]));
        assert_eq!(
            round_points(&harness, &["a", "b", "c", "d", "e"]),
            [
                // Five players lose 0, 4, 7, 10 and 13 points.
                ("a", 0),
                ("b", 0),
                ("c", -20),
                // Did not start, for example because they left.
                ("d", -20),
                ("e", -20),
            ]
        );
        assert_eq!(status(&harness, "a"), (Status::LastChance, 0));
        assert_eq!(status(&harness, "b"), (Status::LastChance, 0));
        assert_eq!(status(&harness, "c"), (Status::Eliminated, 0));
        assert_eq!(status(&harness, "d"), (Status::Eliminated, 0));
        assert_eq!(harness.standing("c").rank, 4);
        assert_eq!(harness.standing("d").rank, 4);
        assert_eq!(status(&harness, "e"), (Status::Playing, 30));
    }

    #[test]
    fn every_dnf_loses_in_last_chance_by_default() {
        let mut harness = playing(
            "",
            vec![last_chance("a"), last_chance("b"), last_chance("c")],
        );
        harness.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 3),
            dnf("c", 1),
        ]));
        assert_eq!(harness.standing("b").status, Status::Eliminated);
        assert_eq!(harness.standing("b").rank, 2);
        assert_eq!(harness.standing("c").rank, 2);
    }

    #[test]
    fn eliminates_without_last_chance() {
        let mut harness = playing(
            r#"{"disable_last_chance": true, "points_repartition": [5, 10], "complex_points_repartition": {}}"#,
            vec![alive("a", 20), alive("b", 8), alive("c", 30)],
        );
        harness.step(round_end(vec![
            finish("a", 30_000),
            finish("b", 31_000),
            finish("c", 32_000),
        ]));
        assert_eq!(status(&harness, "a"), (Status::Playing, 15));
        assert_eq!(status(&harness, "b"), (Status::Eliminated, 0));
        assert_eq!(harness.standing("b").rank, 3);
        assert_eq!(status(&harness, "c"), (Status::Playing, 20));
    }

    #[test]
    fn match_ends_once_the_winners_are_left() {
        let mut harness = playing("", vec![alive("a", 50), last_chance("b"), last_chance("c")]);
        let commands = harness.step(round_end(vec![
            finish("a", 30_000),
            dnf("b", 2),
            dnf("c", 2),
        ]));
        assert_eq!(harness.standing("b").rank, 2);
        assert_eq!(harness.standing("c").rank, 2);
        assert_eq!(status(&harness, "a"), (Status::Winner, 48));
        assert_eq!(harness.standing("a").rank, 1);
        assert_eq!(
            chat(&commands),
            [
                "$f90$i$<b$> takes 2nd place!",
                "$f90$i$<c$> takes 2nd place!",
                "$<a$> wins the match!",
            ]
        );
        assert_eq!(commands.last(), Some(&Command::EndMatch));
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn match_of_one_player_ends_with_the_elimination() {
        // One player loses 1 point as the first place.
        let mut harness = playing("", vec![alive("a", 3)]);
        let commands = harness.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(status(&harness, "a"), (Status::Playing, 2));
        assert!(!commands.contains(&Command::EndMatch));

        harness.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(status(&harness, "a"), (Status::LastChance, 0));
        let commands = harness.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(harness.standing("a").status, Status::Eliminated);
        assert_eq!(harness.standing("a").rank, 1);
        assert_eq!(chat(&commands), ["$f90$i$<a$> takes 1st place!"]);
        assert_eq!(commands.last(), Some(&Command::EndMatch));
    }

    #[test]
    fn fast_forwards_when_nobody_can_keep_points() {
        let players = vec![alive("a", 6), alive("b", 12), eliminated("c", 3)];
        let mut harness = playing("", players.clone());
        let commands = harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        // a has 2 and b 4 points left. The next round costs the first place 4 points.
        assert_eq!(status(&harness, "a"), (Status::LastChance, 0));
        assert_eq!(status(&harness, "b"), (Status::LastChance, 0));
        assert_eq!(
            chat(&commands),
            ["$c00Fast forward: every remaining player is now in Last Chance."]
        );

        let mut harness = playing(r#"{"allow_fast_forward_rounds": false}"#, players);
        harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        assert_eq!(status(&harness, "a"), (Status::Playing, 2));
        assert_eq!(status(&harness, "b"), (Status::Playing, 4));
    }

    #[test]
    fn map_ends_after_rounds_with_lost_points() {
        let mut harness = playing(
            r#"{"rounds_per_map": 1, "complex_points_repartition": {"2": [0, 0]}}"#,
            vec![alive("a", 50), alive("b", 50)],
        );
        // Nobody lost points, so the round does not count.
        let commands = harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        assert!(!commands.contains(&Command::EndMap));

        let commands = harness.step(round_end(vec![finish("a", 30_000), dnf("b", 0)]));
        assert_eq!(commands.last(), Some(&Command::EndMap));
        // Next map, the match goes on.
        assert!(harness.step(Event::MapEnd).is_empty());
    }

    #[test]
    fn ranks_by_points_then_last_chance_then_elimination() {
        let mut harness = playing(
            r#"{"rounds_per_map": 5}"#,
            vec![
                alive("a", 50),
                last_chance("b"),
                eliminated("c", 5),
                alive("d", 60),
                alive("e", 5),
            ],
        );
        // Four players lose 0, 4, 7 and 10 points.
        harness.step(round_end(vec![
            finish("d", 30_000),
            finish("a", 31_000),
            finish("e", 32_000),
            finish("b", 33_000),
        ]));

        let standings: Vec<(&str, u32, Status, i32, i32)> = harness
            .standings
            .players
            .iter()
            .map(|p| {
                (
                    p.account_id.as_str(),
                    p.rank,
                    p.status,
                    p.points,
                    p.round_points,
                )
            })
            .collect();
        assert_eq!(
            standings,
            [
                ("d", 1, Status::Playing, 60, 0),
                ("a", 2, Status::Playing, 46, -4),
                ("e", 3, Status::LastChance, 0, -7),
                ("b", 4, Status::Eliminated, 0, -10),
                ("c", 5, Status::Eliminated, 0, 0),
            ]
        );
        assert_eq!(harness.standings.info, "3 left, 1 / 5 rounds");
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
