//! Rounds mode for the wasm driver.
//!
//! Follows the rules of the Nadeo Rounds mode: finishers get points by their
//! position, the map ends after a number of rounds and the match ends after a
//! number of maps or once a player reached the points limit.
//!
//! The points only live in this mode. They reach the manager as round results
//! and the players through the standings manialink.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tm_driver_sdk::{
    Command, Event, FinishTimeout, Mode, PlayerResult, RaceResult, RoundEnd, RoundFormat,
    standings::{Standing, Standings},
};

tm_driver_sdk::export_mode!(Rounds);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Points by finish position. Finishers beyond the list get the last value.
    pub points_repartition: Vec<i32>,
    /// Valid rounds played on every map. 0 means no limit.
    pub rounds_per_map: u32,
    /// Maps played in the match. 0 means no limit.
    pub maps_per_match: u32,
    /// The match ends once a player reached this many points. 0 disables it.
    pub points_limit: i32,
    /// Keep playing while the leading players are tied.
    pub use_tie_break: bool,
    pub finish_timeout: FinishTimeoutSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            points_repartition: vec![10, 6, 4, 3, 2, 1],
            rounds_per_map: 5,
            maps_per_match: 1,
            points_limit: 0,
            use_tie_break: true,
            finish_timeout: FinishTimeoutSetting::Automatic,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishTimeoutSetting {
    Automatic,
    Disabled,
    Milliseconds(u32),
}

impl From<FinishTimeoutSetting> for FinishTimeout {
    fn from(value: FinishTimeoutSetting) -> Self {
        match value {
            FinishTimeoutSetting::Automatic => FinishTimeout::Automatic,
            FinishTimeoutSetting::Disabled => FinishTimeout::Disabled,
            FinishTimeoutSetting::Milliseconds(ms) => FinishTimeout::Milliseconds(ms),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rounds {
    settings: Settings,
    match_points: BTreeMap<String, i32>,
    map_points: BTreeMap<String, i32>,
    /// Points of the last valid round.
    round_points: BTreeMap<String, i32>,
    names: BTreeMap<String, String>,
    valid_rounds_on_map: u32,
    maps_played: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum PointsLimit {
    NotReached,
    Reached,
    /// Reached or not, the leading players have the same points.
    Tie,
}

impl Mode for Rounds {
    fn init(settings: &str) -> Result<Self, String> {
        let settings = if settings.trim().is_empty() {
            Settings::default()
        } else {
            serde_json::from_str(settings).map_err(|e| format!("Invalid settings: {e}"))?
        };

        Ok(Self {
            settings,
            match_points: BTreeMap::new(),
            map_points: BTreeMap::new(),
            round_points: BTreeMap::new(),
            names: BTreeMap::new(),
            valid_rounds_on_map: 0,
            maps_played: 0,
        })
    }

    fn step(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::MatchStart => {
                self.match_points.clear();
                self.round_points.clear();
                self.maps_played = 0;
                vec![self.standings()]
            }
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active.
            Event::MapStart => {
                self.map_points.clear();
                self.round_points.clear();
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
                self.maps_played += 1;
                if self.match_is_over() {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            Event::MatchEnd => Vec::new(),
        }
    }

    fn snapshot(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    fn restore(snapshot: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(snapshot).map_err(|e| format!("Invalid snapshot: {e}"))
    }
}

impl Rounds {
    fn round_end(&mut self, round_end: RoundEnd) -> Vec<Command> {
        // Forced rounds do not count.
        if round_end.forced {
            return Vec::new();
        }

        let mut finishers: Vec<&RaceResult> = round_end
            .results
            .iter()
            .filter(|r| r.previous_time.is_some())
            .collect();
        // Stable so equal times keep the order of the scores.
        finishers.sort_by_key(|r| r.previous_time);

        self.round_points.clear();
        let mut results = Vec::with_capacity(finishers.len());
        for (rank, finisher) in finishers.into_iter().enumerate() {
            let points = self.points_for(rank);
            self.names
                .insert(finisher.account_id.clone(), finisher.name.clone());
            self.round_points
                .insert(finisher.account_id.clone(), points);
            *self
                .map_points
                .entry(finisher.account_id.clone())
                .or_default() += points;
            *self
                .match_points
                .entry(finisher.account_id.clone())
                .or_default() += points;
            results.push(PlayerResult {
                account_id: finisher.account_id.clone(),
                points,
                position: rank as u32 + 1,
            });
        }

        self.valid_rounds_on_map += 1;

        let mut commands = vec![Command::RoundResult(results), self.standings()];
        if self.points_limit() == PointsLimit::Reached {
            commands.push(Command::EndMatch);
        } else if self.settings.rounds_per_map > 0
            && self.valid_rounds_on_map >= self.settings.rounds_per_map
        {
            commands.push(Command::EndMap);
        }
        commands
    }

    fn standings(&self) -> Command {
        let mut players: Vec<Standing> = self
            .match_points
            .iter()
            .map(|(account_id, &points)| Standing {
                account_id: account_id.clone(),
                name: self.names.get(account_id).cloned().unwrap_or_default(),
                points,
                round_points: self.round_points.get(account_id).copied().unwrap_or(0),
            })
            .collect();
        // Stable so equal points keep a fixed order.
        players.sort_by_key(|p| std::cmp::Reverse(p.points));

        Standings {
            title: "Rounds".into(),
            info: match self.settings.rounds_per_map {
                0 => format!("{} rounds", self.valid_rounds_on_map),
                limit => format!("{} / {limit} rounds", self.valid_rounds_on_map),
            },
            players,
        }
        .command()
    }

    fn points_for(&self, rank: usize) -> i32 {
        let repartition = &self.settings.points_repartition;
        repartition
            .get(rank)
            .or(repartition.last())
            .copied()
            .unwrap_or(0)
    }

    fn points_limit(&self) -> PointsLimit {
        let mut leaders = self.match_points.values().copied().collect::<Vec<_>>();
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

    fn match_is_over(&self) -> bool {
        let points_limit = self.points_limit();
        if points_limit == PointsLimit::Reached {
            true
        } else if self.settings.maps_per_match > 0 {
            self.maps_played >= self.settings.maps_per_match && points_limit != PointsLimit::Tie
        } else {
            // Without any limit the match ends after the first map.
            self.settings.rounds_per_map == 0 && self.settings.points_limit <= 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finish(account_id: &str, time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
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

    fn started(settings: &str) -> Rounds {
        let mut mode = Rounds::init(settings).unwrap();
        mode.step(Event::MatchStart);
        mode.step(Event::MapStart);
        mode
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
            .expect("Every valid round has a result")
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
        let mut mode = Rounds::init(r#"{"finish_timeout": {"milliseconds": 7000}}"#).unwrap();
        assert_eq!(
            mode.step(Event::MapStart)[0],
            Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                7000
            )))
        );
    }

    #[test]
    fn publishes_standings() {
        let mut mode = started(r#"{"points_repartition": [10, 6], "rounds_per_map": 5}"#);
        mode.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
        let commands = mode.step(round_end(vec![finish("b", 30_000), dnf("a")]));

        assert_eq!(
            standings(&commands),
            serde_json::json!({
                "title": "Rounds",
                "info": "2 / 5 rounds",
                "players": [
                    {"account_id": "b", "name": "b", "points": 16, "round_points": 10},
                    {"account_id": "a", "name": "a", "points": 10, "round_points": 0},
                ]
            })
        );
    }

    #[test]
    fn points_by_finish_order() {
        let mut mode = started(r#"{"points_repartition": [10, 6, 4]}"#);

        let commands = mode.step(round_end(vec![
            finish("b", 31_000),
            finish("a", 30_000),
            dnf("d"),
            finish("c", 32_000),
            finish("e", 33_000),
        ]));

        assert_eq!(
            round_result(&commands),
            [
                ("a".into(), 10, 1),
                ("b".into(), 6, 2),
                ("c".into(), 4, 3),
                // Beyond the repartition everyone gets the last value.
                ("e".into(), 4, 4),
            ]
        );
        assert_eq!(mode.match_points["a"], 10);
    }

    #[test]
    fn forced_rounds_do_not_count() {
        let mut mode = started("");
        let commands = mode.step(Event::RoundEnd(RoundEnd {
            forced: true,
            results: vec![finish("a", 30_000)],
        }));
        assert!(commands.is_empty());
        assert_eq!(mode.valid_rounds_on_map, 0);
    }

    #[test]
    fn map_ends_after_rounds_per_map() {
        let mut mode = started(r#"{"rounds_per_map": 2}"#);
        assert!(
            !mode
                .step(round_end(vec![finish("a", 1)]))
                .contains(&Command::EndMap)
        );
        assert!(
            mode.step(round_end(vec![finish("a", 1)]))
                .contains(&Command::EndMap)
        );
        // One map per match by default.
        assert_eq!(mode.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn points_limit_respects_tie_break() {
        let mut mode = started(r#"{"points_repartition": [5, 5], "points_limit": 10}"#);
        mode.step(round_end(vec![finish("a", 1), finish("b", 2)]));
        let commands = mode.step(round_end(vec![finish("a", 1), finish("b", 2)]));
        // Both reached 10 points but they are tied.
        assert!(!commands.contains(&Command::EndMatch));

        let commands = mode.step(round_end(vec![finish("a", 1)]));
        assert!(commands.contains(&Command::EndMatch));
    }

    #[test]
    fn snapshot_round_trip() {
        let mut mode = started(r#"{"rounds_per_map": 3}"#);
        mode.step(round_end(vec![finish("a", 1), finish("b", 2)]));

        let mut restored = Rounds::restore(&mode.snapshot()).unwrap();
        assert_eq!(restored.settings, mode.settings);
        restored.step(round_end(vec![finish("b", 1)]));
        assert_eq!(restored.match_points["b"], 16);
    }

    #[test]
    fn rejects_unknown_settings() {
        assert!(Rounds::init(r#"{"rounds_per_mpa": 3}"#).is_err());
    }
}
