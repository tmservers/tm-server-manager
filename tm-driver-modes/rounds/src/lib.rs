//! Rounds mode for the wasm driver.
//!
//! Follows the rules of the Nadeo Rounds mode: finishers get points by their
//! position, the map ends after a number of rounds and the match ends after a
//! number of maps or once a player reached the points limit.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use tm_driver_sdk::{
    Command, Event, FinishTimeout, Mode, PlayerPoints, RaceResult, RoundEnd, RoundFormat,
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
    /// Players whose round points are shown and need to be reset on the next round.
    round_points_shown: BTreeSet<String>,
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
            round_points_shown: BTreeSet::new(),
            valid_rounds_on_map: 0,
            maps_played: 0,
        })
    }

    fn step(&mut self, event: Event) -> Vec<Command> {
        match event {
            Event::MatchStart(_) => {
                self.match_points.clear();
                self.map_points.clear();
                self.round_points_shown.clear();
                self.valid_rounds_on_map = 0;
                self.maps_played = 0;
                self.round_rules().to_vec()
            }
            Event::MapStart(_) => {
                self.valid_rounds_on_map = 0;
                let mut commands: Vec<Command> = std::mem::take(&mut self.map_points)
                    .into_keys()
                    .map(|account_id| points(account_id, None, Some(0), None))
                    .collect();
                // A config change keeps the match of the modescript going, so the rules
                // of the previous mode might still be active.
                commands.extend(self.round_rules());
                commands
            }
            Event::RoundStart(_) => std::mem::take(&mut self.round_points_shown)
                .into_iter()
                .map(|account_id| points(account_id, Some(0), None, None))
                .collect(),
            Event::RoundEnd(round_end) => self.round_end(round_end),
            Event::MapEnd(_) => {
                self.maps_played += 1;
                if self.match_is_over() {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            Event::MatchEnd(_) => Vec::new(),
            // The scores of a returning player might have been reset by the server.
            Event::PlayerJoin(join) => match self.match_points.get(&join.player.account_id) {
                Some(&match_points) => {
                    let map_points = self
                        .map_points
                        .get(&join.player.account_id)
                        .copied()
                        .unwrap_or(0);
                    vec![points(
                        join.player.account_id,
                        None,
                        Some(map_points),
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

impl Rounds {
    fn round_rules(&self) -> [Command; 2] {
        [
            Command::SetRoundFormat(RoundFormat::SingleAttempt),
            Command::SetFinishTimeout(self.settings.finish_timeout.into()),
        ]
    }

    fn round_end(&mut self, round_end: RoundEnd) -> Vec<Command> {
        // Points of forced rounds do not count. The round points were already reset on round start.
        if round_end.forced {
            return Vec::new();
        }

        let mut finishers: Vec<&RaceResult> = round_end
            .results
            .iter()
            .filter(|r| r.time.is_some())
            .collect();
        // Stable so equal times keep the order of the modescript.
        finishers.sort_by_key(|r| r.time);

        let mut commands = Vec::with_capacity(finishers.len() + 1);
        for (position, result) in finishers.into_iter().enumerate() {
            let round_points = self.points_for(position);
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

            self.round_points_shown.insert(result.account_id.clone());
            commands.push(points(
                result.account_id.clone(),
                Some(round_points),
                Some(map_points),
                Some(*match_points),
            ));
        }

        self.valid_rounds_on_map += 1;

        if self.points_limit() == PointsLimit::Reached {
            commands.push(Command::EndMatch);
        } else if self.settings.rounds_per_map > 0
            && self.valid_rounds_on_map >= self.settings.rounds_per_map
        {
            commands.push(Command::EndMap);
        }

        commands
    }

    fn points_for(&self, position: usize) -> i32 {
        let repartition = &self.settings.points_repartition;
        repartition
            .get(position)
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
    use tm_driver_sdk::{MapStart, MatchStart, RoundStart};

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

    fn started(settings: &str) -> Rounds {
        let mut mode = Rounds::init(settings).unwrap();
        mode.step(Event::MatchStart(MatchStart {
            time: 0,
            players: vec![],
        }));
        mode.step(Event::MapStart(MapStart {
            time: 0,
            map_uid: "uid".into(),
            map_name: "map".into(),
            players: vec![],
        }));
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
            player_points(&commands),
            [
                ("a".into(), Some(10), Some(10)),
                ("b".into(), Some(6), Some(6)),
                ("c".into(), Some(4), Some(4)),
                // Beyond the repartition everyone gets the last value.
                ("e".into(), Some(4), Some(4)),
            ]
        );
    }

    #[test]
    fn round_points_reset_on_next_round() {
        let mut mode = started("");
        mode.step(round_end(vec![finish("a", 30_000)]));

        let commands = mode.step(Event::RoundStart(RoundStart {
            time: 0,
            round: 2,
            players: vec![],
        }));
        assert_eq!(player_points(&commands), [("a".into(), Some(0), None)]);
    }

    #[test]
    fn forced_rounds_do_not_count() {
        let mut mode = started("");
        let commands = mode.step(Event::RoundEnd(RoundEnd {
            time: 0,
            round: 1,
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
        assert_eq!(mode.step(Event::MapEnd(0)), [Command::EndMatch]);
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
        assert_eq!(
            player_points(&restored.step(round_end(vec![finish("b", 1)]))),
            [("b".into(), Some(10), Some(16))]
        );
    }

    #[test]
    fn rejects_unknown_settings() {
        assert!(Rounds::init(r#"{"rounds_per_mpa": 3}"#).is_err());
    }
}
