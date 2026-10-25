//! Time attack mode for the wasm driver.
//!
//! Every map is one time attack round: players restart as often as they like
//! until the time limit runs out and their best time counts. Standings of
//! driver matches sum round points, so the ranking of every map is turned into
//! points. By default the best of N players gets N points, the second N - 1 and
//! so on, which keeps the order of the times.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use tm_driver_sdk::{Command, Event, Mode, PlayerPoints, RaceResult, RoundEnd, RoundFormat};

tm_driver_sdk::export_mode!(TimeAttack);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// How long every map is played in milliseconds.
    pub time_limit_ms: u32,
    /// Maps played in the match. 0 means no limit.
    pub maps_per_match: u32,
    /// Points by rank at the end of every map. Ranks beyond the list get the last value.
    /// When empty the best of N players gets N points, the second N - 1 and so on.
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
    map_points: BTreeMap<String, i32>,
    /// Players whose round and map points are shown and need to be reset on the next map.
    points_shown: BTreeSet<String>,
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
            map_points: BTreeMap::new(),
            points_shown: BTreeSet::new(),
            maps_played: 0,
        })
    }

    fn step(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::MatchStart(_) => {
                self.match_points.clear();
                self.map_points.clear();
                self.points_shown.clear();
                self.maps_played = 0;
                vec![self.round_format()]
            }
            Event::MapStart(_) => {
                self.map_points.clear();
                let mut commands: Vec<Command> = std::mem::take(&mut self.points_shown)
                    .into_iter()
                    .map(|account_id| points(account_id, Some(0), Some(0), None))
                    .collect();
                // A config change keeps the match of the modescript going, so the rules
                // of the previous mode might still be active.
                commands.push(self.round_format());
                commands
            }
            Event::RoundStart(_) => Vec::new(),
            Event::RoundEnd(round_end) => self.round_end(round_end),
            Event::MapEnd(_) => {
                self.maps_played += 1;
                if self.settings.maps_per_match > 0
                    && self.maps_played >= self.settings.maps_per_match
                {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            Event::MatchEnd(_) => Vec::new(),
            // The scores of a returning player might have been reset by the server.
            Event::PlayerJoin(join) => match self.match_points.get(&join.player.account_id) {
                Some(&match_points) => {
                    let map_points = self.map_points.get(&join.player.account_id).copied();
                    vec![points(
                        join.player.account_id,
                        None,
                        map_points,
                        Some(match_points),
                    )]
                }
                None => Vec::new(),
            },
            Event::PlayerLeave(_) => Vec::new(),
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
    fn round_format(&self) -> Command {
        Command::SetRoundFormat(RoundFormat::TimeAttack(self.settings.time_limit_ms))
    }

    fn round_end(&mut self, round_end: RoundEnd) -> Vec<Command> {
        // A forced round was cut short. The map continues with a new time attack round.
        if round_end.forced {
            return Vec::new();
        }

        let participants = round_end.results.len();
        let mut finishers: Vec<&RaceResult> = round_end
            .results
            .iter()
            .filter(|r| r.time.is_some())
            .collect();
        // Stable so equal times keep the order of the modescript.
        finishers.sort_by_key(|r| r.time);

        let mut commands = Vec::with_capacity(finishers.len() + 1);
        for (rank, result) in finishers.into_iter().enumerate() {
            let round_points = self.points_for(rank, participants);
            let map_points = self
                .map_points
                .entry(result.account_id.clone())
                .or_default();
            *map_points += round_points;
            let map_points = *map_points;
            let match_points = self
                .match_points
                .entry(result.account_id.clone())
                .or_default();
            *match_points += round_points;

            self.points_shown.insert(result.account_id.clone());
            commands.push(points(
                result.account_id.clone(),
                Some(round_points),
                Some(map_points),
                Some(*match_points),
            ));
        }

        // Every map is a single time attack round.
        commands.push(Command::EndMap);
        commands
    }

    fn points_for(&self, rank: usize, participants: usize) -> i32 {
        let repartition = &self.settings.points_repartition;
        if repartition.is_empty() {
            // Finishers are a subset of the participants so this is at least 1.
            return i32::try_from(participants - rank).unwrap_or(i32::MAX);
        }
        repartition
            .get(rank)
            .or(repartition.last())
            .copied()
            .unwrap_or(0)
    }
}

fn points(
    account_id: String,
    round_points: Option<i32>,
    map_points: Option<i32>,
    match_points: Option<i32>,
) -> Command {
    Command::SetPlayerPoints(PlayerPoints {
        account_id,
        round_points,
        map_points,
        match_points,
    })
}

#[cfg(test)]
mod tests {
    use tm_driver_sdk::{MapStart, MatchStart};

    use super::*;

    fn finish(account_id: &str, time: u32) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            time: Some(time),
            checkpoints: vec![time],
        }
    }

    fn dnf(account_id: &str) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            time: None,
            checkpoints: vec![],
        }
    }

    fn round_end(results: Vec<RaceResult>) -> Event {
        Event::RoundEnd(RoundEnd {
            time: 0,
            round: 1,
            forced: false,
            results,
        })
    }

    fn map_start() -> Event {
        Event::MapStart(MapStart {
            time: 0,
            map_uid: "uid".into(),
            map_name: "map".into(),
            players: vec![],
        })
    }

    fn started(settings: &str) -> TimeAttack {
        let mut mode = TimeAttack::init(settings).unwrap();
        mode.step(Event::MatchStart(MatchStart {
            time: 0,
            players: vec![],
        }));
        mode.step(map_start());
        mode
    }

    fn player_points(commands: &[Command]) -> Vec<(String, Option<i32>, Option<i32>)> {
        commands
            .iter()
            .filter_map(|c| match c {
                Command::SetPlayerPoints(p) => {
                    Some((p.account_id.clone(), p.round_points, p.match_points))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn sets_the_round_format() {
        let mut mode = TimeAttack::init(r#"{"time_limit_ms": 60000}"#).unwrap();
        let format = || Command::SetRoundFormat(RoundFormat::TimeAttack(60_000));
        assert_eq!(
            mode.step(Event::MatchStart(MatchStart {
                time: 0,
                players: vec![]
            })),
            [format()]
        );
        assert_eq!(mode.step(map_start()), [format()]);
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
            player_points(&commands),
            [
                ("a".into(), Some(3), Some(3)),
                ("b".into(), Some(2), Some(2)),
            ]
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
            player_points(&commands),
            [
                ("a".into(), Some(10), Some(10)),
                ("b".into(), Some(5), Some(5)),
                ("c".into(), Some(5), Some(5)),
            ]
        );
    }

    #[test]
    fn forced_rounds_replay_the_map() {
        let mut mode = started("");
        let commands = mode.step(Event::RoundEnd(RoundEnd {
            time: 0,
            round: 1,
            forced: true,
            results: vec![finish("a", 40_000)],
        }));
        assert!(commands.is_empty());
    }

    #[test]
    fn points_add_up_over_maps() {
        let mut mode = started(r#"{"maps_per_match": 2}"#);
        mode.step(round_end(vec![finish("a", 40_000), finish("b", 41_000)]));
        assert!(mode.step(Event::MapEnd(0)).is_empty());

        // The points of the last map are reset on the next map.
        assert_eq!(
            player_points(&mode.step(map_start())),
            [("a".into(), Some(0), None), ("b".into(), Some(0), None)]
        );

        let commands = mode.step(round_end(vec![finish("b", 39_000), finish("a", 40_000)]));
        assert_eq!(
            player_points(&commands),
            [
                ("b".into(), Some(2), Some(3)),
                ("a".into(), Some(1), Some(3))
            ]
        );
        assert_eq!(mode.step(Event::MapEnd(0)), [Command::EndMatch]);
    }

    #[test]
    fn snapshot_round_trip() {
        let mut mode = started(r#"{"maps_per_match": 2}"#);
        mode.step(round_end(vec![finish("a", 40_000)]));

        let mut restored = TimeAttack::restore(&mode.snapshot()).unwrap();
        assert_eq!(restored.settings, mode.settings);
        restored.step(Event::MapEnd(0));
        assert_eq!(restored.step(Event::MapEnd(0)), [Command::EndMatch]);
    }

    #[test]
    fn rejects_invalid_settings() {
        assert!(TimeAttack::init(r#"{"time_limit": 300}"#).is_err());
        assert!(TimeAttack::init(r#"{"time_limit_ms": 0}"#).is_err());
    }
}
