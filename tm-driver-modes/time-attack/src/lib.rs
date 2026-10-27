//! Time attack mode for the wasm driver.
//!
//! Every map is one time attack round: players restart as often as they like
//! until the time limit runs out and their best time counts. The ranking of
//! every map is turned into points so the standings can add up several maps.
//! By default the best of N finishers gets N points, the second N - 1 and so
//! on, which keeps the order of the times.
//!
//! Nobody ties: equal best times go to the better times at the checkpoints of
//! those races and finally to the name, players with the same points by their
//! race on the last map.

use serde::Deserialize;
use tm_driver_sdk::{
    Command, CountedRace, Event, Mode, RoundEnd, RoundFormat, Standings,
    settings::{WarmUpSetting, maps_per_match},
};

tm_driver_sdk::export_mode!(TimeAttack);

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// How long every map is played in milliseconds.
    pub time_limit_ms: u32,
    /// Maps played in the match. By default the playlist once, one map without a
    /// playlist. 0 means no limit.
    pub maps_per_match: Option<u32>,
    /// Points by rank at the end of every map. Ranks beyond the list get the last value.
    /// When empty the best of N finishers gets N points, the second N - 1 and so on.
    pub points_repartition: Vec<i32>,
    /// The warmup before the time attack round of every map.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            time_limit_ms: 300_000,
            maps_per_match: None,
            points_repartition: Vec::new(),
            warmup: WarmUpSetting::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeAttack {
    settings: Settings,
}

impl Mode for TimeAttack {
    const NAME: &'static str = "Time Attack";

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        if settings.time_limit_ms == 0 {
            return Err("The time limit has to be at least one millisecond.".into());
        }
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
                    Command::SetRoundFormat(RoundFormat::TimeAttack(self.settings.time_limit_ms)),
                    Command::SetWarmUp(self.settings.warmup.into()),
                ]
            }
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            Event::MapEnd => {
                let maps = maps_per_match(self.settings.maps_per_match);
                if maps > 0 && standings.maps_played >= maps {
                    standings.crown_leaders();
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            // Everyone who races shows up in the standings before the first round ends.
            Event::RaceStart(account_id) => {
                standings.join(&account_id);
                standings.list_newcomers();
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
    }
}

impl TimeAttack {
    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // A forced round was cut short. The map continues with a new time attack round.
        if round_end.forced {
            return Vec::new();
        }

        // Everyone who raced shows up in the standings, with points or without.
        for result in round_end.results.iter().filter(|r| r.started) {
            standings.join(&result.account_id);
        }

        let finishers = tm_driver_sdk::race_order(
            round_end
                .results
                .iter()
                .filter(|r| r.best_time.is_some())
                .map(|r| r.account_id.as_str()),
            &round_end.results,
            CountedRace::Best,
        );
        for (rank, account_id) in finishers.iter().enumerate() {
            standings.award(account_id, self.points_for(rank, finishers.len()));
        }

        standings.rounds_on_map += 1;
        standings.rank(&round_end.results, CountedRace::Best);
        standings.place_round(&round_end.results, CountedRace::Best);
        standings.info = self.info(standings);
        // Every map is a single time attack round.
        vec![Command::EndMap]
    }

    fn info(&self, standings: &Standings) -> String {
        let map = standings.maps_played + 1;
        match maps_per_match(self.settings.maps_per_match) {
            0 => format!("Map {map}"),
            maps => format!("Map {map} / {maps}"),
        }
    }

    fn points_for(&self, rank: usize, finishers: usize) -> i32 {
        let repartition = &self.settings.points_repartition;
        if repartition.is_empty() {
            return i32::try_from(finishers - rank).unwrap_or(i32::MAX);
        }
        repartition
            .get(rank)
            .or(repartition.last())
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use tm_driver_sdk::{Label, RaceResult, WarmUp, testing::Harness};

    use super::*;

    /// In time attack rounds the best race counts, not the last one.
    fn finish(account_id: &str, best_time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            started: true,
            round_checkpoints: vec![best_time + 1_000],
            previous_time: Some(best_time + 1_000),
            previous_checkpoints: vec![],
            best_time: Some(best_time),
            best_checkpoints: vec![best_time],
        }
    }

    fn dnf(account_id: &str) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            started: true,
            round_checkpoints: vec![10_000],
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

    fn started(settings: &str) -> Harness<TimeAttack> {
        let mut harness = Harness::new(settings).unwrap();
        harness.step(Event::MatchStart);
        harness.step(Event::MapStart);
        harness
    }

    /// Account, position and points of the map of every player in the order of the
    /// standings. Every map is a single round.
    fn map_points(harness: &Harness<TimeAttack>) -> Vec<(&str, u32, i32)> {
        harness
            .standings
            .entries
            .iter()
            .map(|p| {
                let round = harness.round_entry(&p.account_id).map_or(0, |r| r.score);
                (p.account_id.as_str(), p.position, round)
            })
            .collect()
    }

    #[test]
    fn sets_the_round_format_and_warmup_on_map_start() {
        let mut harness = Harness::<TimeAttack>::new(
            r#"{"time_limit_ms": 60000, "warmup": {"rounds": 1, "timeout_ms": 5000}}"#,
        )
        .unwrap();
        assert_eq!(
            harness.step(Event::MapStart),
            [
                Command::SetRoundFormat(RoundFormat::TimeAttack(60_000)),
                Command::SetWarmUp(WarmUp {
                    rounds: 1,
                    duration_ms: 0,
                    timeout_ms: Some(5000),
                }),
            ]
        );
        assert_eq!(harness.standings.info, "Map 1 / 1");
    }

    #[test]
    fn default_points_keep_the_order_of_times() {
        let mut harness = started("");
        let commands = harness.step(round_end(vec![
            finish("b", 41_000),
            finish("a", 40_000),
            dnf("c"),
        ]));

        assert_eq!(
            map_points(&harness),
            [("a", 1, 2), ("b", 2, 1), ("c", 3, 0)]
        );
        assert_eq!(commands, [Command::EndMap]);
    }

    #[test]
    fn points_repartition() {
        let mut harness = started(r#"{"points_repartition": [10, 5]}"#);
        harness.step(round_end(vec![
            finish("a", 40_000),
            finish("b", 41_000),
            finish("c", 42_000),
        ]));
        assert_eq!(
            map_points(&harness),
            [("a", 1, 10), ("b", 2, 5), ("c", 3, 5)]
        );
    }

    #[test]
    fn equal_best_times_go_to_the_better_checkpoints() {
        let mut harness = started("");
        // b passed the last checkpoint of the best race earlier.
        harness.step(round_end(vec![
            RaceResult {
                best_checkpoints: vec![20_500, 40_000],
                ..finish("a", 40_000)
            },
            RaceResult {
                best_checkpoints: vec![20_400, 40_000],
                ..finish("b", 40_000)
            },
        ]));
        assert_eq!(map_points(&harness), [("b", 1, 2), ("a", 2, 1)]);
    }

    #[test]
    fn forced_rounds_replay_the_map() {
        let mut harness = started("");
        let commands = harness.step(Event::RoundEnd(RoundEnd {
            forced: true,
            results: vec![finish("a", 40_000)],
        }));
        assert!(commands.is_empty());
        assert!(harness.standings.entries.is_empty());
    }

    #[test]
    fn points_add_up_over_maps() {
        let mut harness = started(r#"{"maps_per_match": 2}"#);
        harness.step(round_end(vec![finish("a", 40_000), finish("b", 41_000)]));
        assert!(harness.step(Event::MapEnd).is_empty());

        harness.step(Event::MapStart);
        assert_eq!(harness.standings.info, "Map 2 / 2");
        harness.step(round_end(vec![finish("b", 39_000), finish("a", 40_000)]));
        assert_eq!(harness.standing("a").score, 3);
        assert_eq!(harness.standing("b").score, 3);
        assert_eq!(
            harness.round_entry("b").map(|r| (r.position, r.score)),
            Some((1, 2))
        );

        // Tied on points, b won the last map.
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
        assert_eq!(harness.standing("b").position, 1);
        assert_eq!(harness.standing("b").label, Some(Label::winner()));
        assert_eq!(harness.standing("a").position, 2);
        assert_eq!(harness.standing("a").label, None);
    }

    #[test]
    fn plays_the_playlist_once() {
        let mut harness = Harness::<TimeAttack>::new("").unwrap();
        harness.map_count = 3;
        harness.step(Event::MatchStart);
        harness.step(Event::MapStart);
        assert_eq!(harness.standings.info, "Map 1 / 3");
        for _ in 0..2 {
            harness.step(round_end(vec![finish("a", 40_000)]));
            assert!(harness.step(Event::MapEnd).is_empty());
            harness.step(Event::MapStart);
        }
        harness.step(round_end(vec![finish("a", 40_000)]));
        assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(TimeAttack::from_json(r#"{"time_limit": 300}"#).is_err());
        assert!(TimeAttack::from_json(r#"{"time_limit_ms": 0}"#).is_err());
    }
}
