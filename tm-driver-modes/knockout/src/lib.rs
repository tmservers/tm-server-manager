//! Knockout mode for the wasm driver.
//!
//! Follows the rules of the Nadeo Knockout mode: the players race rounds and the
//! last players of every round are eliminated until one player is left, who wins.
//! Everyone without a finish is eliminated as well, even beyond the eliminations
//! of the round. The more players are left, the more are eliminated per round.
//!
//! The first rounds of the match eliminate nobody. The players who start a race
//! before the first round with eliminations ended take part, everyone else
//! cannot spawn anymore.
//!
//! Participants and their statuses live in the standings the host keeps. Players
//! still in the match are ranked by their last race, eliminated players keep the
//! rank they were eliminated with.

use std::cmp::Reverse;

use serde::Deserialize;
use tm_driver_sdk::{
    CanSpawn, Command, Event, Label, Mode, RaceResult, RoundEnd, RoundFormat, Standings, Status,
    Tone,
    settings::{FinishTimeoutSetting, WarmUpSetting},
};

tm_driver_sdk::export_mode!(Knockout);

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Rounds at the start of the match in which nobody is eliminated.
    pub rounds_without_elimination: u32,
    /// One more player is eliminated per round above each of these numbers of
    /// players left: `[4, 16, 16]` eliminates 4 players per round above 16 players,
    /// 2 above 4 players and 1 from there on.
    pub extra_eliminations_above: Vec<u32>,
    /// Valid rounds played on every map. 0 means no limit.
    pub rounds_per_map: u32,
    pub finish_timeout: FinishTimeoutSetting,
    /// The warmup before the first round of every map.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rounds_without_elimination: 1,
            extra_eliminations_above: vec![4, 16, 16],
            rounds_per_map: 0,
            finish_timeout: FinishTimeoutSetting::Milliseconds(5_000),
            warmup: WarmUpSetting::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Knockout {
    settings: Settings,
}

impl Mode for Knockout {
    const NAME: &'static str = "Knockout";

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        Ok(Self { settings })
    }

    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        let commands = self.handle(standings, event);
        // The labels and the info follow from the standings alone.
        self.label(standings);
        standings.info = self.info(standings);
        commands
    }
}

impl Knockout {
    fn handle(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active.
            Event::MapStart => vec![
                Command::SetRoundFormat(RoundFormat::SingleAttempt(
                    self.settings.finish_timeout.into(),
                )),
                Command::SetWarmUp(self.settings.warmup.into()),
            ],
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            Event::MapEnd => {
                if self.match_over(standings) {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            // Players take part from their first race on, so the standings show them
            // during the first round.
            Event::RaceStart(account_id) => {
                if self.registration_open(standings) && standings.player(&account_id).is_none() {
                    standings.join(&account_id);
                    rank(standings);
                }
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
    }

    /// The time of the last round for the players in the match like in the scores
    /// table of the original, `K.O.` for eliminated players and `Winner` once the
    /// match is over. The labels follow from the standings alone.
    fn label(&self, standings: &mut Standings) {
        let match_over = self.match_over(standings);
        for player in &mut standings.entries {
            player.label = match player.status {
                Status::Playing if match_over => Some(Label::winner()),
                Status::Playing => player
                    .time
                    .map(|time| Label::new(race_time(time), Tone::Neutral)),
                Status::Eliminated => Some(Label::new("K.O.", Tone::Negative)),
                Status::Spectator => None,
            };
        }
    }

    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // The host counted the round unless it was forced.
        let round = standings.rounds_played + u32::from(round_end.forced);
        // Players can join until the first round with eliminations ended. Their race
        // starts brought them in already, unless e.g. the driver restarted.
        if round <= self.settings.rounds_without_elimination + 1
            || standings.participants().next().is_none()
        {
            register(standings, &round_end.results);
        }
        if round_end.forced || standings.participants().next().is_none() {
            return self.block_late_players(standings, &round_end.results);
        }

        let mut commands = self.eliminate(standings, round, &round_end.results);
        standings.rounds_on_map += 1;
        commands.extend(self.block_late_players(standings, &round_end.results));
        rank(standings);
        standings.place_round(&round_end.results, |r| r.previous_time);

        if self.match_over(standings) {
            commands.extend(winner_message(standings).map(Command::ChatMessage));
            commands.push(Command::EndMatch);
        } else if self.settings.rounds_per_map > 0
            && standings.rounds_on_map >= self.settings.rounds_per_map
        {
            commands.push(Command::EndMap);
        }
        commands
    }

    /// Eliminates the last players of the round and everyone without a finish, who
    /// get the rank of their race. Everyone in the match gets the time of the round.
    fn eliminate(
        &self,
        standings: &mut Standings,
        round: u32,
        results: &[RaceResult],
    ) -> Vec<Command> {
        let order = race_order(standings, results);
        let without_finish = order.iter().filter(|(_, time)| time.is_none()).count();
        let eliminated = match self.round_eliminations(round, order.len()) {
            0 => 0,
            // Everyone without a finish is out, the slowest finishers fill up the
            // eliminations of the round.
            eliminations => eliminations.max(without_finish),
        };
        // Even without a finish, the player who got the furthest wins.
        let survivors = order.len() - eliminated.min(order.len().saturating_sub(1));

        let mut commands = Vec::new();
        for (index, (account_id, time)) in order.into_iter().enumerate() {
            let player = standings
                .player_mut(&account_id)
                .expect("Only players in the match raced");
            player.time = time;
            if index < survivors {
                continue;
            }
            let position = index as u32 + 1;
            player.status = Status::Eliminated;
            player.position = position;
            commands.push(Command::ChatMessage(format!(
                "$f90$i$<{}$> takes {} place!",
                tm_driver_sdk::name(&account_id),
                tm_driver_sdk::ordinal(position)
            )));
            commands.push(Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn: false,
            }));
        }
        commands
    }

    /// Players who could still take part are not blocked.
    fn block_late_players(
        &self,
        standings: &mut Standings,
        results: &[RaceResult],
    ) -> Vec<Command> {
        if self.registration_open(standings) {
            Vec::new()
        } else {
            standings.block_others(results)
        }
    }

    /// Players can join until the first round with eliminations ended, and as long
    /// as nobody takes part.
    fn registration_open(&self, standings: &Standings) -> bool {
        standings.rounds_played <= self.settings.rounds_without_elimination
            || standings.participants().next().is_none()
    }

    /// One player is left and nobody can join anymore.
    fn match_over(&self, standings: &Standings) -> bool {
        !self.registration_open(standings) && standings.in_match().count() <= 1
    }

    /// The eliminations of a round with this many players left. Players without a
    /// finish can add to them.
    fn round_eliminations(&self, round: u32, left: usize) -> usize {
        if round > self.settings.rounds_without_elimination {
            self.eliminations(left)
        } else {
            0
        }
    }

    /// One player per round, plus one for every number of `extra_eliminations_above`
    /// below the players left. One player always stays.
    fn eliminations(&self, left: usize) -> usize {
        let extra = self
            .settings
            .extra_eliminations_above
            .iter()
            .filter(|&&above| (above as usize) < left)
            .count();
        (1 + extra).min(left.saturating_sub(1))
    }

    /// The estimated rounds of the match from the round that runs or comes next on,
    /// unless players without a finish speed it up.
    fn rounds(&self, round: u32, mut left: usize) -> u32 {
        // The rounds it takes to eliminate everyone but one of the players left.
        let mut eliminating: u32 = 0;
        while left > 1 {
            left -= self.eliminations(left);
            eliminating += 1;
        }
        // The first of them is the next round with eliminations.
        round.max(self.settings.rounds_without_elimination + 1) + eliminating.saturating_sub(1)
    }

    /// The round that runs or comes next with its eliminations, e.g.
    /// `Round 2 / 22, 64 left, 4 K.O.`.
    fn info(&self, standings: &Standings) -> String {
        if self.match_over(standings) {
            return "Match over".into();
        }
        let round = standings.rounds_played + 1;
        let left = standings.in_match().count();
        let rounds = format!("Round {round} / {}", self.rounds(round, left));
        let eliminations = match self.round_eliminations(round, left) {
            0 => "no K.O.".to_string(),
            eliminations => format!("{eliminations} K.O."),
        };
        if standings.participants().next().is_none() {
            format!("{rounds}, {eliminations}")
        } else {
            format!("{rounds}, {left} left, {eliminations}")
        }
    }
}

/// Everyone who started a race takes part while players can join.
fn register(standings: &mut Standings, results: &[RaceResult]) {
    for result in results.iter().filter(|r| r.started) {
        standings.join(&result.account_id);
    }
}

/// The players in the match by the time of their last round, everyone without a
/// finish after them.
fn rank(standings: &mut Standings) {
    standings.rank_by(|player| (player.time.is_none(), player.time));
}

/// The players in the match by their race with its time, the best first:
/// finishers by time, then everyone else by the checkpoints they passed and the
/// players who did not start last. Ties keep the order of the standings.
fn race_order(standings: &Standings, results: &[RaceResult]) -> Vec<(String, Option<u32>)> {
    let mut order: Vec<(String, Option<u32>, Option<u32>)> = standings
        .in_match()
        .map(|player| {
            let race = results
                .iter()
                .find(|r| r.account_id == player.account_id && r.started);
            (
                player.account_id.clone(),
                race.and_then(|r| r.previous_time),
                race.map(|r| r.progress),
            )
        })
        .collect();
    // Stable so ties keep the order of the standings.
    order.sort_by_key(|&(_, time, progress)| (time.is_none(), time, Reverse(progress)));
    order
        .into_iter()
        .map(|(account_id, time, _)| (account_id, time))
        .collect()
}

/// The last player in the match wins.
fn winner_message(standings: &Standings) -> Option<String> {
    standings.in_match().next().map(|winner| {
        format!(
            "$<{}$> wins the match!",
            tm_driver_sdk::name(&winner.account_id)
        )
    })
}

/// A race time the way the game shows it, e.g. `1:02.345`.
fn race_time(milliseconds: u32) -> String {
    format!(
        "{}:{:02}.{:03}",
        milliseconds / 60_000,
        milliseconds / 1_000 % 60,
        milliseconds % 1_000
    )
}

#[cfg(test)]
mod tests {
    use tm_driver_sdk::{FinishTimeout, WarmUp, testing::Harness};

    use super::*;

    fn finish(account_id: &str, time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
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

    /// The players finish in this order, one second apart.
    fn finishers(players: &[&str]) -> Vec<RaceResult> {
        players
            .iter()
            .enumerate()
            .map(|(index, player)| finish(player, 30_000 + 1_000 * index as u32))
            .collect()
    }

    fn round_end(results: Vec<RaceResult>) -> Event {
        Event::RoundEnd(RoundEnd {
            forced: false,
            results,
        })
    }

    fn race_start(account_id: &str) -> Event {
        Event::RaceStart(account_id.into())
    }

    fn started(settings: &str) -> Harness<Knockout> {
        let mut harness = Harness::new(settings).unwrap();
        harness.step(Event::MatchStart);
        harness.step(Event::MapStart);
        harness
    }

    /// A match after its round without elimination, in which these players took
    /// part. The next round eliminates.
    fn playing(settings: &str, players: &[&str]) -> Harness<Knockout> {
        let mut harness = started(settings);
        harness.step(round_end(finishers(players)));
        harness
    }

    /// Account, position, status and label of every player in the order of the
    /// standings.
    fn ranking(harness: &Harness<Knockout>) -> Vec<(&str, u32, Status, &str)> {
        harness
            .standings
            .entries
            .iter()
            .map(|p| {
                let label = p.label.as_ref().map_or("", |label| label.text.as_str());
                (p.account_id.as_str(), p.position, p.status, label)
            })
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
    fn sets_the_round_format_and_warmup_on_map_start() {
        let mut harness = Harness::<Knockout>::new("").unwrap();
        assert_eq!(
            harness.step(Event::MapStart),
            [
                Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                    5_000
                ))),
                Command::SetWarmUp(WarmUp {
                    rounds: 0,
                    duration_ms: 0,
                    timeout_ms: None,
                }),
            ]
        );
    }

    #[test]
    fn eliminations_grow_with_the_players_left() {
        let eliminations = |settings: &str, left: &[usize]| -> Vec<usize> {
            let mode = Knockout::from_json(settings).unwrap();
            left.iter().map(|&left| mode.eliminations(left)).collect()
        };
        assert_eq!(
            eliminations("", &[64, 17, 16, 5, 4, 2, 1, 0]),
            [4, 4, 2, 2, 1, 1, 0, 0]
        );
        // 0 adds an elimination however many players are left.
        assert_eq!(
            eliminations(r#"{"extra_eliminations_above": [0, 8]}"#, &[9, 8, 3, 2]),
            [3, 2, 2, 1]
        );
        assert_eq!(
            eliminations(r#"{"extra_eliminations_above": []}"#, &[64, 2]),
            [1, 1]
        );
    }

    #[test]
    fn estimates_the_rounds_of_the_match() {
        let mode = Knockout::from_json("").unwrap();
        // 1 round without elimination, 12 rounds down to 16 players, 6 down to 4 and 3
        // down to the winner.
        assert_eq!(mode.rounds(1, 64), 22);
        assert_eq!(mode.rounds(2, 64), 22);
        assert_eq!(mode.rounds(3, 60), 22);
        // Two players need a round with an elimination.
        assert_eq!(mode.rounds(1, 2), 2);
        assert_eq!(mode.rounds(1, 0), 2);
    }

    #[test]
    fn players_take_part_from_their_first_race() {
        let mut harness = started("");
        assert_eq!(harness.standings.info, "Round 1 / 2, no K.O.");
        assert!(harness.step(race_start("a")).is_empty());
        harness.step(race_start("b"));
        assert_eq!(
            ranking(&harness),
            [("a", 1, Status::Playing, ""), ("b", 1, Status::Playing, "")]
        );
        assert_eq!(harness.standings.info, "Round 1 / 2, 2 left, no K.O.");
    }

    #[test]
    fn the_first_round_eliminates_nobody() {
        let mut harness = started("");
        let commands = harness.step(round_end(vec![
            finish("b", 31_000),
            dnf("c", 2),
            finish("a", 30_000),
            spectator("s"),
        ]));
        assert!(commands.is_empty());
        // Everyone who started takes part, ranked by the race.
        assert_eq!(
            ranking(&harness),
            [
                ("a", 1, Status::Playing, "0:30.000"),
                ("b", 2, Status::Playing, "0:31.000"),
                ("c", 3, Status::Playing, ""),
            ]
        );
        assert_eq!(harness.standings.info, "Round 2 / 3, 3 left, 1 K.O.");
    }

    #[test]
    fn players_join_until_the_first_round_with_eliminations_ended() {
        let mut harness = playing("", &["a", "b", "c", "d"]);
        // e joins in time and takes part. Five players, two are eliminated.
        let mut results = finishers(&["a", "b", "c", "d"]);
        results.extend([finish("e", 29_000), spectator("s")]);
        let commands = harness.step(round_end(results));
        assert_eq!(harness.standing("e").position, 1);
        assert_eq!(blocked(&commands), ["c", "d", "s"]);
        assert_eq!(harness.standing("s").status, Status::Spectator);

        // Too late, f does not take part and cannot spawn anymore.
        harness.step(race_start("f"));
        assert!(harness.standings.player("f").is_none());
        let mut results = finishers(&["e", "a", "b"]);
        results.push(finish("f", 28_000));
        let commands = harness.step(round_end(results));
        assert_eq!(blocked(&commands), ["b", "f"]);
        assert_eq!(harness.standing("f").status, Status::Spectator);
        assert_eq!(harness.standing("f").position, 0);
    }

    #[test]
    fn the_last_players_are_eliminated() {
        let mut harness = playing("", &["a", "b", "c", "d", "e", "f"]);
        // Six players, two are eliminated.
        let commands = harness.step(round_end(finishers(&["b", "a", "c", "d", "e", "f"])));
        assert_eq!(
            chat(&commands),
            [
                "$f90$i$<e$> takes 5th place!",
                "$f90$i$<f$> takes 6th place!"
            ]
        );
        assert_eq!(blocked(&commands), ["e", "f"]);
        assert_eq!(
            ranking(&harness),
            [
                ("b", 1, Status::Playing, "0:30.000"),
                ("a", 2, Status::Playing, "0:31.000"),
                ("c", 3, Status::Playing, "0:32.000"),
                ("d", 4, Status::Playing, "0:33.000"),
                ("e", 5, Status::Eliminated, "K.O."),
                ("f", 6, Status::Eliminated, "K.O."),
            ]
        );
        assert_eq!(harness.standings.info, "Round 3 / 5, 4 left, 1 K.O.");

        // Places follow the race and show who was eliminated.
        let places: Vec<(&str, u32, Status)> = harness
            .round
            .iter()
            .map(|p| (p.account_id.as_str(), p.position, p.status))
            .collect();
        assert_eq!(
            places[4..],
            [("e", 5, Status::Eliminated), ("f", 6, Status::Eliminated)]
        );
    }

    #[test]
    fn everyone_without_a_finish_is_eliminated() {
        let mut harness = playing("", &["a", "b", "c", "d", "e", "f"]);
        // Two eliminations, but three players did not finish. f left the server.
        let commands = harness.step(round_end(vec![
            finish("a", 30_000),
            dnf("c", 2),
            finish("b", 31_000),
            dnf("e", 0),
            finish("d", 29_000),
        ]));
        // By the checkpoints they passed, players who did not start last.
        assert_eq!(
            chat(&commands),
            [
                "$f90$i$<c$> takes 4th place!",
                "$f90$i$<e$> takes 5th place!",
                "$f90$i$<f$> takes 6th place!"
            ]
        );
        let in_match: Vec<&str> = harness
            .standings
            .in_match()
            .map(|p| p.account_id.as_str())
            .collect();
        assert_eq!(in_match, ["d", "a", "b"]);
    }

    #[test]
    fn the_slowest_finishers_fill_up_the_eliminations() {
        let mut harness = playing("", &["a", "b", "c", "d", "e", "f"]);
        let mut results = finishers(&["a", "b", "c", "d", "e"]);
        results.push(dnf("f", 1));
        let commands = harness.step(round_end(results));
        assert_eq!(blocked(&commands), ["e", "f"]);
    }

    #[test]
    fn the_last_player_wins() {
        let mut harness = playing("", &["a", "b"]);
        let commands = harness.step(round_end(finishers(&["b", "a"])));
        assert_eq!(
            chat(&commands),
            ["$f90$i$<a$> takes 2nd place!", "$<b$> wins the match!"]
        );
        assert_eq!(commands.last(), Some(&Command::EndMatch));
        assert_eq!(
            ranking(&harness),
            [
                ("b", 1, Status::Playing, "Winner"),
                ("a", 2, Status::Eliminated, "K.O.")
            ]
        );
        assert_eq!(harness.standings.info, "Match over");
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn the_player_who_got_the_furthest_wins_without_finishers() {
        let mut harness = playing("", &["a", "b", "c"]);
        let commands = harness.step(round_end(vec![dnf("a", 1), dnf("b", 3), dnf("c", 2)]));
        assert_eq!(commands.last(), Some(&Command::EndMatch));
        assert_eq!(
            ranking(&harness),
            [
                ("b", 1, Status::Playing, "Winner"),
                ("c", 2, Status::Eliminated, "K.O."),
                ("a", 3, Status::Eliminated, "K.O."),
            ]
        );
    }

    #[test]
    fn a_single_player_wins_once_nobody_can_join() {
        let mut harness = playing("", &["a"]);
        assert_eq!(harness.standings.info, "Round 2 / 2, 1 left, no K.O.");
        let commands = harness.step(round_end(vec![dnf("a", 1)]));
        assert_eq!(
            commands,
            [
                Command::ChatMessage("$<a$> wins the match!".into()),
                Command::EndMatch
            ]
        );
    }

    #[test]
    fn eliminates_from_the_first_round_without_rounds_without_elimination() {
        let mut harness = started(r#"{"rounds_without_elimination": 0}"#);
        assert_eq!(harness.standings.info, "Round 1 / 1, no K.O.");
        let commands = harness.step(round_end(finishers(&["a", "b", "c"])));
        assert_eq!(chat(&commands), ["$f90$i$<c$> takes 3rd place!"]);
    }

    #[test]
    fn forced_rounds_do_not_count() {
        let forced = |results| {
            Event::RoundEnd(RoundEnd {
                forced: true,
                results,
            })
        };
        let mut harness = playing("", &["a", "b", "c"]);
        // Nobody is eliminated, but starters still join.
        let commands = harness.step(forced(vec![finish("d", 31_000), dnf("a", 1)]));
        assert!(commands.is_empty());
        assert_eq!(harness.standings.in_match().count(), 4);
        assert_eq!(harness.standings.rounds_on_map, 1);
        assert_eq!(harness.standings.info, "Round 2 / 4, 4 left, 1 K.O.");
    }

    #[test]
    fn rounds_without_players_do_not_close_the_registration() {
        let mut harness = started("");
        for _ in 0..3 {
            harness.step(round_end(vec![spectator("s")]));
        }
        assert!(harness.standings.entries.is_empty());
        assert_eq!(harness.standings.rounds_on_map, 0);

        // Like in the original, the rounds without elimination are over though.
        harness.step(round_end(finishers(&["a", "b"])));
        assert_eq!(harness.standings.in_match().count(), 1);
        assert_eq!(harness.standing("a").label, Some(Label::winner()));
    }

    #[test]
    fn map_ends_after_rounds_per_map() {
        let mut harness = started(r#"{"rounds_per_map": 2}"#);
        let commands = harness.step(round_end(finishers(&["a", "b", "c"])));
        assert!(!commands.contains(&Command::EndMap));
        let commands = harness.step(round_end(finishers(&["a", "b", "c"])));
        assert_eq!(commands.last(), Some(&Command::EndMap));

        // The match goes on with the next map.
        assert!(harness.step(Event::MapEnd).is_empty());
        harness.step(Event::MapStart);
        assert_eq!(harness.standings.rounds_on_map, 0);
        assert_eq!(harness.standings.info, "Round 3 / 3, 2 left, 1 K.O.");
    }

    #[test]
    fn info_follows_the_eliminations() {
        let mut harness = started("");
        let players: Vec<String> = (0..64).map(|i| format!("p{i:02}")).collect();
        let players: Vec<&str> = players.iter().map(String::as_str).collect();
        for player in &players {
            harness.step(race_start(player));
        }
        assert_eq!(harness.standings.info, "Round 1 / 22, 64 left, no K.O.");
        harness.step(round_end(finishers(&players)));
        assert_eq!(harness.standings.info, "Round 2 / 22, 64 left, 4 K.O.");
        harness.step(round_end(finishers(&players)));
        assert_eq!(harness.standings.info, "Round 3 / 22, 60 left, 4 K.O.");
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(Knockout::from_json(r#"{"rounds_without_eliminations": 1}"#).is_err());
        assert!(Knockout::from_json(r#"{"extra_eliminations_above": [-1]}"#).is_err());
        assert!(Knockout::from_json(r#"{"finish_timeout": "automatic"}"#).is_ok());
    }

    #[test]
    fn race_times() {
        let times: Vec<String> = [0, 42_123, 62_005, 3_723_456]
            .into_iter()
            .map(race_time)
            .collect();
        assert_eq!(times, ["0:00.000", "0:42.123", "1:02.005", "62:03.456"]);
    }
}
