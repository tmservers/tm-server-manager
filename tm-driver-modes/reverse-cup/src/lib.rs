//! Reverse cup mode for the wasm driver.
//!
//! Follows the rules of the community ReverseCup mode: every player starts with
//! points and loses points by their finish position. A player who runs out of
//! points is in last chance and is eliminated by losing the most points of a
//! round. The match ends once only the winners are left.
//!
//! The players who start a race before the first round counts take part in the
//! match with their starting points. Everyone else cannot spawn anymore.
//!
//! The original mode skips a round that would put everyone in last chance
//! anyway. This mode does that right after the previous round, so the round is
//! not started at all.
//!
//! Participants, their points and statuses live in the standings the host keeps.
//! Players with points are playing, players in last chance have 0 points and
//! eliminated players keep the rank they were eliminated with. Nobody ties: equal
//! races go to the better times at the checkpoints and finally to the name,
//! players with the same points by their race in the last round.

use std::collections::BTreeMap;

use serde::Deserialize;
use tm_driver_sdk::{
    CanSpawn, Command, CountedRace, Entry, Event, Label, Mode, RaceResult, RoundEnd, RoundFormat,
    Standings, Status, Tone,
    settings::{FinishTimeoutSetting, WarmUpSetting},
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
}

impl ReverseCup {
    fn handle(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active.
            Event::MapStart => {
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
                    standings.list_newcomers();
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
            self.register(standings, &round_end.results);
        }
        if round_end.invalid
            || standings.participants().next().is_none()
            || self.match_over(standings)
        {
            standings.list_newcomers();
            return self.block_late_players(standings, &round_end.results);
        }
        let mut commands = Vec::new();

        let losses = self.losses(standings, &round_end.results);
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
                tm_driver_sdk::name(&account_id),
                tm_driver_sdk::ordinal(position)
            )));
            commands.push(Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn: false,
            }));
        }

        if losses.iter().any(|&(_, loss)| loss < 0) {
            standings.rounds_on_map += 1;
        }
        commands.extend(self.block_late_players(standings, &round_end.results));

        let match_over = self.match_over(standings);
        if !match_over && self.fast_forward(standings) {
            commands.push(Command::ChatMessage(
                "$c00Fast forward: every remaining player is now in Last Chance.".into(),
            ));
        }
        standings.rank(&round_end.results, CountedRace::Last);
        standings.place_round(&round_end.results, CountedRace::Last);
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

    /// Players can take part until the first round counts.
    fn registration_open(&self, standings: &Standings) -> bool {
        standings.rounds_on_map == 0 && standings.maps_played == 0
    }

    /// Everyone who started a race takes part while registration is open.
    fn register(&self, standings: &mut Standings, results: &[RaceResult]) {
        for result in results.iter().filter(|r| r.started) {
            if standings.player(&result.account_id).is_none() {
                standings.join(&result.account_id).score = self.settings.starting_points;
            }
        }
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

    /// Points lost by every player still in the match in the order of the race:
    /// finishers by their finish position, everyone without a finish the dnf points.
    fn losses(&self, standings: &Standings, results: &[RaceResult]) -> Vec<(String, i32)> {
        let order = tm_driver_sdk::race_order(
            standings.in_match().map(|p| p.account_id.as_str()),
            results,
            CountedRace::Last,
        );
        // Players who did not start, for example because they left, have no race.
        let race = |account_id: &str| {
            results
                .iter()
                .find(|r| r.account_id == account_id && r.started)
        };
        let last_chance =
            |account_id: &str| standings.player(account_id).is_some_and(in_last_chance);
        let least_checkpoints = order
            .iter()
            .filter(|account_id| last_chance(account_id))
            .filter_map(|account_id| race(account_id))
            .filter(|race| race.previous_time.is_none())
            .map(|race| race.round_checkpoints.len())
            .min();

        let repartition = self.repartition(standings);
        let mut finishers = 0;
        order
            .into_iter()
            .map(|account_id| {
                let loss = match race(&account_id) {
                    Some(race) if race.previous_time.is_some() => {
                        let loss = repartition
                            .get(finishers)
                            .or(repartition.last())
                            .copied()
                            .unwrap_or(0);
                        finishers += 1;
                        loss
                    }
                    race => match (self.settings.last_chance_dnf_mode, race) {
                        (LastChanceDnfMode::OnlyLeastCheckpoints, Some(race))
                            if last_chance(&account_id)
                                && Some(race.round_checkpoints.len()) != least_checkpoints =>
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

    /// Only the winners are left. A single participant plays until eliminated.
    fn match_over(&self, standings: &Standings) -> bool {
        let left = standings.in_match().count();
        match standings.participants().count() {
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
        .map(|p| format!("$<{}$>", tm_driver_sdk::name(&p.account_id)))
        .collect();
    match winners.as_slice() {
        [] => None,
        [winner] => Some(format!("{winner} wins the match!")),
        _ => Some(format!("{} win the match!", winners.join(", "))),
    }
}

#[cfg(test)]
mod tests {
    use tm_driver_sdk::{FinishTimeout, WarmUp, testing::Harness};

    use super::*;

    fn finish(account_id: &str, time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            started: true,
            round_checkpoints: vec![time / 2, time / 2, time],
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
            round_checkpoints: (1..=progress)
                .map(|checkpoint| checkpoint * 10_000)
                .collect(),
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
            invalid: false,
            results,
        })
    }

    fn standing(account_id: &str, status: Status, points: i32, position: u32) -> Entry {
        Entry {
            account_id: account_id.into(),
            position,
            score: points,
            time: None,
            status,
            label: None,
        }
    }

    fn alive(account_id: &str, points: i32) -> Entry {
        standing(account_id, Status::Playing, points, 0)
    }

    fn last_chance(account_id: &str) -> Entry {
        standing(account_id, Status::Playing, 0, 0)
    }

    /// Players in last chance still play without points.
    const IN_LAST_CHANCE: (Status, i32) = (Status::Playing, 0);

    fn eliminated(account_id: &str, rank: u32) -> Entry {
        standing(account_id, Status::Eliminated, 0, rank)
    }

    fn started(settings: &str) -> Harness<ReverseCup> {
        let mut harness = Harness::new(settings).unwrap();
        harness.step(Event::MatchStart);
        harness.step(Event::MapStart);
        harness
    }

    /// A match whose participants already reached these standings. A map was played
    /// already, so registration is closed.
    fn playing(settings: &str, players: Vec<Entry>) -> Harness<ReverseCup> {
        let mut harness = started(settings);
        harness.standings.entries = players;
        harness.standings.maps_played = 1;
        harness
    }

    fn race_start(account_id: &str) -> Event {
        Event::RaceStart(account_id.into())
    }

    fn status(harness: &Harness<ReverseCup>, account_id: &str) -> (Status, i32) {
        let player = harness.standing(account_id);
        (player.status, player.score)
    }

    fn round_points<'a>(
        harness: &'a Harness<ReverseCup>,
        accounts: &[&'a str],
    ) -> Vec<(&'a str, i32)> {
        accounts
            .iter()
            .map(|&account_id| {
                let round = harness.round_entry(account_id).map_or(0, |r| r.score);
                (account_id, round)
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
        let mut harness = Harness::<ReverseCup>::new(
            r#"{"finish_timeout": {"milliseconds": 7000}, "warmup": {"rounds": 1, "duration_ms": 60000}}"#,
        )
        .unwrap();
        assert_eq!(
            harness.step(Event::MapStart),
            [
                Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                    7000
                ))),
                Command::SetWarmUp(WarmUp {
                    rounds: 1,
                    duration_ms: 60_000,
                    timeout_ms: None,
                }),
            ]
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
            .participants()
            .map(|p| p.account_id.as_str())
            .collect();
        assert_eq!(participants, ["a", "b"]);
        assert_eq!(blocked(&commands), ["s"]);
        assert_eq!(harness.standing("s").status, Status::Spectator);
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
        assert_eq!(harness.standing("c").status, Status::Spectator);
        assert_eq!(harness.standing("c").position, 0);
        assert_eq!(harness.standings.info, "2 left, 2 / 5 rounds");
    }

    #[test]
    fn players_take_part_from_their_first_race() {
        let mut harness = started("");
        assert!(harness.step(race_start("a")).is_empty());
        harness.step(race_start("b"));
        assert_eq!(status(&harness, "a"), (Status::Playing, 100));
        // Without a position until the first round.
        assert_eq!(harness.standing("a").position, 0);
        assert_eq!(harness.standings.info, "2 left, 0 / 5 rounds");

        harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        assert_eq!(status(&harness, "b"), (Status::Playing, 92));

        // The first round counted, so later players do not take part anymore.
        harness.step(race_start("c"));
        assert!(harness.standings.player("c").is_none());
        let commands = harness.step(round_end(vec![
            finish("c", 29_000),
            finish("a", 30_000),
            finish("b", 31_000),
        ]));
        assert_eq!(blocked(&commands), ["c"]);
        assert_eq!(harness.standing("c").status, Status::Spectator);
    }

    #[test]
    fn invalid_rounds_do_not_count() {
        let invalid = |results| {
            Event::RoundEnd(RoundEnd {
                invalid: true,
                results,
            })
        };
        // Players who started take part, but nobody loses points and nobody is blocked
        // while registration is open.
        let mut harness = started("");
        assert!(
            harness
                .step(invalid(vec![finish("a", 30_000), spectator("s")]))
                .is_empty()
        );
        assert_eq!(status(&harness, "a"), (Status::Playing, 100));
        assert!(harness.standings.player("s").is_none());

        let mut harness = playing("", vec![alive("a", 50)]);
        let commands = harness.step(invalid(vec![finish("a", 30_000), finish("c", 31_000)]));
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
    fn equal_times_go_to_the_better_checkpoints() {
        let mut harness = playing(
            r#"{"points_repartition": [1, 2], "complex_points_repartition": {}}"#,
            vec![alive("a", 50), alive("b", 50)],
        );
        // b passed the last checkpoint earlier and loses less.
        harness.step(round_end(vec![
            RaceResult {
                previous_checkpoints: vec![10_000, 20_500, 30_000],
                ..finish("a", 30_000)
            },
            RaceResult {
                previous_checkpoints: vec![10_100, 20_400, 30_000],
                ..finish("b", 30_000)
            },
        ]));
        assert_eq!(round_points(&harness, &["a", "b"]), [("a", -2), ("b", -1)]);
        assert_eq!(harness.standing("b").position, 1);
    }

    #[test]
    fn repartition_by_players_left() {
        let mode =
            ReverseCup::from_json(r#"{"complex_points_repartition": {"2, 3": [5, 10]}}"#).unwrap();
        let mut standings = Standings {
            entries: vec![alive("a", 50), alive("b", 50), alive("c", 50)],
            ..Default::default()
        };
        assert_eq!(mode.repartition(&standings), [5, 10]);

        let mut mode = ReverseCup::from_json(
            r#"{"points_repartition": [1, 2, 3], "complex_points_repartition": {}}"#,
        )
        .unwrap();
        standings.entries = vec![
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
        standings.entries.remove(0);
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
        assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
        assert_eq!(harness.standing("b").label, Some(last_chance_label()));
        assert_eq!(status(&harness, "c"), (Status::Eliminated, 0));
        assert_eq!(harness.standing("c").label, None);
        assert_eq!(harness.standing("c").position, 3);
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
        assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
        assert_eq!(status(&harness, "c"), IN_LAST_CHANCE);
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
        assert_eq!(status(&harness, "a"), IN_LAST_CHANCE);
        assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
        assert_eq!(status(&harness, "c"), (Status::Eliminated, 0));
        assert_eq!(status(&harness, "d"), (Status::Eliminated, 0));
        // c raced, d did not.
        assert_eq!(harness.standing("c").position, 4);
        assert_eq!(harness.standing("d").position, 5);
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
        // b got further than c.
        assert_eq!(harness.standing("b").position, 2);
        assert_eq!(harness.standing("c").position, 3);
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
        assert_eq!(harness.standing("b").position, 3);
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
        // Equal races go by name.
        assert_eq!(harness.standing("b").position, 2);
        assert_eq!(harness.standing("c").position, 3);
        assert_eq!(status(&harness, "a"), (Status::Playing, 48));
        assert_eq!(harness.standing("a").label, Some(Label::winner()));
        assert_eq!(harness.standing("a").position, 1);
        assert_eq!(
            chat(&commands),
            [
                "$f90$i$<b$> takes 2nd place!",
                "$f90$i$<c$> takes 3rd place!",
                "$<a$> wins the match!",
            ]
        );
        assert_eq!(commands.last(), Some(&Command::EndMatch));
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn nobody_wins_while_players_can_join() {
        let mut harness = started(r#"{"number_of_winners": 2}"#);
        harness.step(race_start("a"));
        harness.step(race_start("b"));
        assert_eq!(harness.standing("a").label, None);
        assert_eq!(harness.standing("b").label, None);
    }

    #[test]
    fn match_of_one_player_ends_with_the_elimination() {
        // One player loses 1 point as the first place.
        let mut harness = playing("", vec![alive("a", 3)]);
        let commands = harness.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(status(&harness, "a"), (Status::Playing, 2));
        assert!(!commands.contains(&Command::EndMatch));

        harness.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(status(&harness, "a"), IN_LAST_CHANCE);
        let commands = harness.step(round_end(vec![finish("a", 30_000)]));
        assert_eq!(harness.standing("a").status, Status::Eliminated);
        assert_eq!(harness.standing("a").position, 1);
        assert_eq!(chat(&commands), ["$f90$i$<a$> takes 1st place!"]);
        assert_eq!(commands.last(), Some(&Command::EndMatch));
    }

    #[test]
    fn fast_forwards_when_nobody_can_keep_points() {
        let players = vec![alive("a", 6), alive("b", 12), eliminated("c", 3)];
        let mut harness = playing("", players.clone());
        let commands = harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        // a has 2 and b 4 points left. The next round costs the first place 4 points.
        assert_eq!(status(&harness, "a"), IN_LAST_CHANCE);
        assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
        assert_eq!(harness.standing("b").label, Some(last_chance_label()));
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
            .entries
            .iter()
            .map(|p| {
                let round = harness.round_entry(&p.account_id).map_or(0, |r| r.score);
                (p.account_id.as_str(), p.position, p.status, p.score, round)
            })
            .collect();
        assert_eq!(
            standings,
            [
                ("d", 1, Status::Playing, 60, 0),
                ("a", 2, Status::Playing, 46, -4),
                // In last chance.
                ("e", 3, Status::Playing, 0, -7),
                ("b", 4, Status::Eliminated, 0, -10),
                ("c", 5, Status::Eliminated, 0, 0),
            ]
        );
        assert_eq!(harness.standings.info, "3 left, 1 / 5 rounds");

        // Places follow the race, c was eliminated before and did not race.
        let places: Vec<(&str, u32, Status)> = harness
            .round
            .iter()
            .map(|p| (p.account_id.as_str(), p.position, p.status))
            .collect();
        assert_eq!(
            places,
            [
                ("d", 1, Status::Playing),
                ("a", 2, Status::Playing),
                ("e", 3, Status::Playing),
                ("b", 4, Status::Eliminated)
            ]
        );
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(ReverseCup::from_json(r#"{"starting_point": 100}"#).is_err());
        assert!(
            ReverseCup::from_json(r#"{"complex_points_repartition": {"two": [4, 8]}}"#).is_err()
        );
        assert!(
            ReverseCup::from_json(r#"{"last_chance_dnf_mode": "only_least_checkpoints"}"#).is_ok()
        );
    }
}
