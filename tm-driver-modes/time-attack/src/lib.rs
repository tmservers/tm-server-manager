//! Time attack mode for the wasm driver.
//!
//! Every map is one time attack round: players restart as often as they like
//! until the time limit runs out and their best time counts. Standings of
//! driver matches sum round points, so the ranking of every map is turned into
//! points. By default the best of N finishers gets N points, the second N - 1
//! and so on, which keeps the order of the times.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tm_driver_sdk::{Command, Event, Mode, PlayerResult, RaceResult, RoundEnd, RoundFormat};

tm_driver_sdk::export_mode!(TimeAttack);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// How long every map is played in milliseconds.
    pub time_limit_ms: u32,
    /// Maps played in the match. 0 means no limit.
    pub maps_per_match: u32,
    /// Points by rank at the end of every map. Ranks beyond the list get the last value.
    /// When empty the best of N finishers gets N points, the second N - 1 and so on.
    pub points_repartition: Vec<i32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            time_limit_ms: 300_000,
            maps_per_match: 1,
            points_repartition: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeAttack {
    settings: Settings,
    match_points: BTreeMap<String, i32>,
    maps_played: u32,
}

impl Mode for TimeAttack {
    fn init(settings: &str) -> Result<Self, String> {
        let settings: Settings = if settings.trim().is_empty() {
            Settings::default()
        } else {
            serde_json::from_str(settings).map_err(|e| format!("Invalid settings: {e}"))?
        };
        if settings.time_limit_ms == 0 {
            return Err("The time limit has to be at least one millisecond.".into());
        }

        Ok(Self {
            settings,
            match_points: BTreeMap::new(),
            maps_played: 0,
        })
    }

    fn step(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::MatchStart => {
                self.match_points.clear();
                self.maps_played = 0;
                Vec::new()
            }
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active.
            Event::MapStart => vec![Command::SetRoundFormat(RoundFormat::TimeAttack(
                self.settings.time_limit_ms,
            ))],
            Event::RoundStart => Vec::new(),
            Event::RoundEnd(round_end) => self.round_end(round_end),
            Event::MapEnd => {
                self.maps_played += 1;
                if self.settings.maps_per_match > 0
                    && self.maps_played >= self.settings.maps_per_match
                {
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

impl TimeAttack {
    fn round_end(&mut self, round_end: RoundEnd) -> Vec<Command> {
        // A forced round was cut short. The map continues with a new time attack round.
        if round_end.forced {
            return Vec::new();
        }

        let mut finishers: Vec<&RaceResult> = round_end
            .results
            .iter()
            .filter(|r| r.best_time.is_some())
            .collect();
        // Stable so equal times keep the order of the scores.
        finishers.sort_by_key(|r| r.best_time);

        let finisher_count = finishers.len();
        let mut results = Vec::with_capacity(finisher_count);
        for (rank, finisher) in finishers.into_iter().enumerate() {
            let points = self.points_for(rank, finisher_count);
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

        // Every map is a single time attack round.
        vec![Command::RoundResult(results), Command::EndMap]
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
    use super::*;

    /// In time attack rounds the best race counts, not the last one.
    fn finish(account_id: &str, best_time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
            previous_time: Some(best_time + 1_000),
            previous_checkpoints: vec![],
            best_time: Some(best_time),
            best_checkpoints: vec![best_time],
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

    fn started(settings: &str) -> TimeAttack {
        let mut mode = TimeAttack::init(settings).unwrap();
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

    #[test]
    fn sets_the_round_format_on_map_start() {
        let mut mode = TimeAttack::init(r#"{"time_limit_ms": 60000}"#).unwrap();
        assert_eq!(
            mode.step(Event::MapStart),
            [Command::SetRoundFormat(RoundFormat::TimeAttack(60_000))]
        );
    }

    #[test]
    fn default_points_keep_the_order_of_times() {
        let mut mode = started("");
        let commands = mode.step(round_end(vec![
            finish("b", 41_000),
            finish("a", 40_000),
            dnf("c"),
        ]));

        assert_eq!(
            round_result(&commands),
            [("a".into(), 2, 1), ("b".into(), 1, 2)]
        );
        assert_eq!(commands.last(), Some(&Command::EndMap));
    }

    #[test]
    fn points_repartition() {
        let mut mode = started(r#"{"points_repartition": [10, 5]}"#);
        let commands = mode.step(round_end(vec![
            finish("a", 40_000),
            finish("b", 41_000),
            finish("c", 42_000),
        ]));
        assert_eq!(
            round_result(&commands),
            [("a".into(), 10, 1), ("b".into(), 5, 2), ("c".into(), 5, 3)]
        );
    }

    #[test]
    fn forced_rounds_replay_the_map() {
        let mut mode = started("");
        let commands = mode.step(Event::RoundEnd(RoundEnd {
            forced: true,
            results: vec![finish("a", 40_000)],
        }));
        assert!(commands.is_empty());
    }

    #[test]
    fn points_add_up_over_maps() {
        let mut mode = started(r#"{"maps_per_match": 2}"#);
        mode.step(round_end(vec![finish("a", 40_000), finish("b", 41_000)]));
        assert!(mode.step(Event::MapEnd).is_empty());

        mode.step(Event::MapStart);
        mode.step(round_end(vec![finish("b", 39_000), finish("a", 40_000)]));
        assert_eq!(mode.match_points["a"], 3);
        assert_eq!(mode.match_points["b"], 3);
        assert_eq!(mode.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn snapshot_round_trip() {
        let mut mode = started(r#"{"maps_per_match": 2}"#);
        mode.step(round_end(vec![finish("a", 40_000)]));

        let mut restored = TimeAttack::restore(&mode.snapshot()).unwrap();
        assert_eq!(restored.settings, mode.settings);
        restored.step(Event::MapEnd);
        assert_eq!(restored.step(Event::MapEnd), [Command::EndMatch]);
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(TimeAttack::init(r#"{"time_limit": 300}"#).is_err());
        assert!(TimeAttack::init(r#"{"time_limit_ms": 0}"#).is_err());
    }
}
