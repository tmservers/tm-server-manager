//! Rounds mode for the wasm driver.
//!
//! Follows the rules of the Nadeo Rounds mode: finishers get points by their
//! position, the map ends after a number of rounds and the match ends after a
//! number of maps or once a player reached the points limit.
//!
//! The points live in the standings the host keeps and hands over with every
//! event. The mode only applies its rules to them.

use serde::Deserialize;
use tm_driver_sdk::{
    Command, Event, Mode, RaceResult, RoundEnd, RoundFormat, Standings,
    settings::{FinishTimeoutSetting, WarmUpSetting},
};

tm_driver_sdk::export_mode!(Rounds);

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Points by finish position. Finishers beyond the list get the last value.
    pub points_repartition: Vec<i32>,
    /// Valid rounds played on every map. 0 means no limit.
    pub rounds_per_map: u32,
    /// Maps played in the match. By default the playlist once, one map without a
    /// playlist. 0 means no limit.
    pub maps_per_match: Option<u32>,
    /// The match ends once a player reached this many points. 0 disables it.
    pub points_limit: i32,
    /// Keep playing while the leading players are tied.
    pub use_tie_break: bool,
    pub finish_timeout: FinishTimeoutSetting,
    /// The warmup before the first round of every map.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            points_repartition: vec![10, 6, 4, 3, 2, 1],
            rounds_per_map: 5,
            maps_per_match: None,
            points_limit: 0,
            use_tie_break: true,
            finish_timeout: FinishTimeoutSetting::Automatic,
            warmup: WarmUpSetting::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Rounds {
    settings: Settings,
}

#[derive(Debug, PartialEq, Eq)]
enum PointsLimit {
    NotReached,
    Reached,
    /// Reached or not, the leading players have the same points.
    Tie,
}

impl Mode for Rounds {
    const NAME: &'static str = "Rounds";

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        Ok(Self { settings })
    }

    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
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
                if self.match_is_over(standings) {
                    standings.crown_leaders();
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            // Everyone who races shows up in the standings before the first round ends.
            Event::RaceStart(player) => {
                standings.join(&player.account_id, &player.name);
                standings.rank();
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
    }
}

impl Rounds {
    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // Forced rounds do not count.
        if round_end.forced {
            return Vec::new();
        }

        // Everyone who raced shows up in the standings, with points or without.
        for result in round_end
            .results
            .iter()
            .filter(|r| r.started || r.previous_time.is_some())
        {
            standings.join(&result.account_id, &result.name);
        }

        let mut finishers: Vec<&RaceResult> = round_end
            .results
            .iter()
            .filter(|r| r.previous_time.is_some())
            .collect();
        // Stable so equal times keep the order of the scores.
        finishers.sort_by_key(|r| r.previous_time);
        for (rank, finisher) in finishers.into_iter().enumerate() {
            standings
                .join(&finisher.account_id, &finisher.name)
                .award(self.points_for(rank));
        }

        standings.rounds_on_map += 1;
        standings.rank();
        standings.place_round(&round_end.results, |r| r.previous_time);
        standings.info = self.info(standings);

        if self.points_limit(standings) == PointsLimit::Reached {
            standings.crown_leaders();
            vec![Command::EndMatch]
        } else if self.settings.rounds_per_map > 0
            && standings.rounds_on_map >= self.settings.rounds_per_map
        {
            vec![Command::EndMap]
        } else {
            Vec::new()
        }
    }

    fn info(&self, standings: &Standings) -> String {
        let rounds = match self.settings.rounds_per_map {
            0 => format!("{} rounds", standings.rounds_on_map),
            limit => format!("{} / {limit} rounds", standings.rounds_on_map),
        };
        let map = standings.maps_played + 1;
        match maps_per_match(self.settings.maps_per_match, standings) {
            1 => rounds,
            0 => format!("Map {map}, {rounds}"),
            maps => format!("Map {map} / {maps}, {rounds}"),
        }
    }

    fn points_for(&self, rank: usize) -> i32 {
        let repartition = &self.settings.points_repartition;
        repartition
            .get(rank)
            .or(repartition.last())
            .copied()
            .unwrap_or(0)
    }

    fn points_limit(&self, standings: &Standings) -> PointsLimit {
        let mut leaders: Vec<i32> = standings.players.iter().map(|p| p.points).collect();
        leaders.sort_unstable_by(|a, b| b.cmp(a));

        if self.settings.use_tie_break && leaders.len() > 1 && leaders[0] == leaders[1] {
            return PointsLimit::Tie;
        }
        match leaders.first() {
            Some(&best) if self.settings.points_limit > 0 && best >= self.settings.points_limit => {
                PointsLimit::Reached
            }
            _ => PointsLimit::NotReached,
        }
    }

    fn match_is_over(&self, standings: &Standings) -> bool {
        let points_limit = self.points_limit(standings);
        if points_limit == PointsLimit::Reached {
            true
        } else if let maps @ 1.. = maps_per_match(self.settings.maps_per_match, standings) {
            standings.maps_played >= maps && points_limit != PointsLimit::Tie
        } else {
            // Without any limit the match ends after the first map.
            self.settings.rounds_per_map == 0 && self.settings.points_limit <= 0
        }
    }
}

/// The setting or the maps of the playlist.
fn maps_per_match(setting: Option<u32>, standings: &Standings) -> u32 {
    setting.unwrap_or(standings.map_count.max(1))
}

#[cfg(test)]
mod tests {
    use tm_driver_sdk::{FinishTimeout, Label, Player, WarmUp, testing::Harness};

    use super::*;

    fn finish(account_id: &str, time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
            started: true,
            progress: 1,
            previous_time: Some(time),
            previous_checkpoints: vec![time],
            best_time: Some(time),
            best_checkpoints: vec![time],
        }
    }

    fn dnf(account_id: &str) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
            started: true,
            progress: 1,
            previous_time: None,
            previous_checkpoints: vec![],
            best_time: None,
            best_checkpoints: vec![],
        }
    }

    fn round_end(results: Vec<RaceResult>) -> Event {
        Event::RoundEnd(RoundEnd {
            forced: false,
            results,
        })
    }

    fn started(settings: &str) -> Harness<Rounds> {
        let mut harness = Harness::new(settings).unwrap();
        harness.step(Event::MatchStart);
        harness.step(Event::MapStart);
        harness
    }

    /// Account, rank, points and round points of every player in the order of the standings.
    fn scores(harness: &Harness<Rounds>) -> Vec<(&str, u32, i32, i32)> {
        harness
            .standings
            .players
            .iter()
            .map(|p| (p.account_id.as_str(), p.rank, p.points, p.round_points))
            .collect()
    }

    #[test]
    fn sets_the_round_format_and_warmup_on_map_start() {
        let mut harness = Harness::<Rounds>::new(
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
    fn points_by_finish_order() {
        let mut harness = started(r#"{"points_repartition": [10, 6, 4]}"#);
        harness.step(round_end(vec![
            finish("b", 31_000),
            finish("a", 30_000),
            dnf("d"),
            finish("c", 32_000),
            finish("e", 33_000),
        ]));

        assert_eq!(
            scores(&harness),
            [
                ("a", 1, 10, 10),
                ("b", 2, 6, 6),
                // Beyond the repartition everyone gets the last value.
                ("c", 3, 4, 4),
                ("e", 3, 4, 4),
                // Raced without a finish.
                ("d", 5, 0, 0),
            ]
        );
        assert_eq!(harness.standings.info, "1 / 5 rounds");

        // Places follow the race, also for the finishers who got the same points.
        let places: Vec<(&str, u32)> = harness
            .standings
            .players
            .iter()
            .map(|p| (p.account_id.as_str(), p.round_position))
            .collect();
        assert_eq!(places, [("a", 1), ("b", 2), ("c", 3), ("e", 4), ("d", 5)]);
    }

    #[test]
    fn points_add_up_and_round_points_start_over() {
        let mut harness = started(r#"{"points_repartition": [10, 6]}"#);
        harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        harness.step(round_end(vec![finish("b", 30_000), dnf("a")]));

        assert_eq!(scores(&harness), [("b", 1, 16, 10), ("a", 2, 10, 0)]);
        assert_eq!(harness.standing("b").map_points, 16);
    }

    #[test]
    fn players_show_up_when_they_start() {
        let mut harness = started("");
        assert!(
            harness
                .step(Event::RaceStart(Player {
                    account_id: "a".into(),
                    name: "$f00a".into(),
                }))
                .is_empty()
        );
        assert_eq!(scores(&harness), [("a", 1, 0, 0)]);
        assert_eq!(harness.standing("a").name, "$f00a");
    }

    #[test]
    fn forced_rounds_do_not_count() {
        let mut harness = started("");
        let commands = harness.step(Event::RoundEnd(RoundEnd {
            forced: true,
            results: vec![finish("a", 30_000)],
        }));
        assert!(commands.is_empty());
        assert!(harness.standings.players.is_empty());
        assert_eq!(harness.standings.rounds_on_map, 0);
    }

    #[test]
    fn map_ends_after_rounds_per_map() {
        let mut harness = started(r#"{"rounds_per_map": 2}"#);
        assert!(
            !harness
                .step(round_end(vec![finish("a", 1), finish("b", 2)]))
                .contains(&Command::EndMap)
        );
        assert!(
            harness
                .step(round_end(vec![finish("a", 1), finish("b", 2)]))
                .contains(&Command::EndMap)
        );

        // One map per match by default. The leader wins.
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
        assert_eq!(harness.standing("a").label, Some(Label::winner()));
        assert_eq!(harness.standing("b").label, None);
    }

    #[test]
    fn map_points_start_over_on_every_map() {
        let mut harness = started(r#"{"rounds_per_map": 1, "maps_per_match": 2}"#);
        harness.step(round_end(vec![finish("a", 1)]));
        assert!(harness.step(Event::MapEnd).is_empty());

        harness.step(Event::MapStart);
        assert_eq!(harness.standing("a").map_points, 0);
        harness.step(round_end(vec![finish("a", 1)]));
        assert_eq!(harness.standing("a").points, 20);
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn plays_the_playlist_once() {
        let mut harness = Harness::<Rounds>::new(r#"{"rounds_per_map": 1}"#).unwrap();
        harness.map_count = 2;
        harness.step(Event::MatchStart);
        harness.step(Event::MapStart);
        assert_eq!(harness.standings.info, "Map 1 / 2, 0 / 1 rounds");
        harness.step(round_end(vec![finish("a", 1)]));
        assert!(harness.step(Event::MapEnd).is_empty());

        harness.step(Event::MapStart);
        harness.step(round_end(vec![finish("a", 1)]));
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn points_limit_respects_tie_break() {
        let mut harness = started(r#"{"points_repartition": [5, 5], "points_limit": 10}"#);
        harness.step(round_end(vec![finish("a", 1), finish("b", 2)]));
        let commands = harness.step(round_end(vec![finish("a", 1), finish("b", 2)]));
        // Both reached 10 points but they are tied.
        assert!(!commands.contains(&Command::EndMatch));

        let commands = harness.step(round_end(vec![finish("a", 1)]));
        assert!(commands.contains(&Command::EndMatch));
        assert_eq!(harness.standing("a").label, Some(Label::winner()));
    }

    #[test]
    fn rejects_unknown_settings() {
        assert!(Rounds::from_json(r#"{"rounds_per_mpa": 3}"#).is_err());
    }
}
