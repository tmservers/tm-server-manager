//! Conversions between the tm-server-types and the generated WIT types.
//!
//! The commands of a mode are matched exhaustively without a catch-all arm.
//! Adding a command on either side fails to compile until it is mapped here.

use std::{cmp::Reverse, collections::BTreeMap};

use tm_server_types::{
    base::{Player, RoundTime},
    event::{DriverPlayerResult, DriverRoundResult, Scores},
    method::{
        DriverCanSpawn, DriverCommand, DriverFinishTimeout, DriverLayer, DriverLayerKind,
        DriverRoundFormat, DriverStanding, DriverStandings, DriverStatus,
    },
};

use crate::bindings::tmservers::driver::types::{
    CanSpawn, Command, FinishTimeout, Layer, RaceResult, RoundFormat, Standing, Standings, Status,
};

/// Race times of every player of a scores callback together with the progress
/// of the players who started in the round.
pub(crate) fn race_results(
    scores: &Scores,
    round_progress: &BTreeMap<String, u32>,
) -> Vec<RaceResult> {
    let mut results: Vec<RaceResult> = scores
        .players
        .iter()
        .map(|player| race_result(player, round_progress.get(&player.account_id).copied()))
        .collect();

    // A player who left before the scores were sent still took part in the round.
    for (account_id, &progress) in round_progress {
        if !results
            .iter()
            .any(|result| &result.account_id == account_id)
        {
            results.push(RaceResult {
                account_id: account_id.clone(),
                name: String::new(),
                started: true,
                progress,
                previous_time: None,
                previous_checkpoints: Vec::new(),
                best_time: None,
                best_checkpoints: Vec::new(),
            });
        }
    }
    results
}

fn race_result(player: &Player, progress: Option<u32>) -> RaceResult {
    RaceResult {
        account_id: player.account_id.clone(),
        name: player.name.clone(),
        started: progress.is_some(),
        progress: progress.unwrap_or(0),
        previous_time: race_time(&player.previous_racetime),
        previous_checkpoints: player.previous_race_checkpoints.clone(),
        best_time: race_time(&player.best_racetime),
        best_checkpoints: player.best_race_checkpoints.clone(),
    }
}

fn race_time(time: &RoundTime) -> Option<u32> {
    match time {
        RoundTime::Time(time) if *time > 0 => Some(*time),
        RoundTime::Time(_) | RoundTime::None => None,
    }
}

/// The commands of a mode for the server.
pub(crate) fn commands(commands: Vec<Command>) -> Vec<DriverCommand> {
    commands
        .into_iter()
        .map(|command| match command {
            Command::SetRoundFormat(format) => DriverCommand::SetRoundFormat(round_format(format)),
            Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn,
            }) => DriverCommand::SetCanSpawn(DriverCanSpawn {
                account_id,
                can_spawn,
            }),
            Command::EndMap => DriverCommand::EndMap,
            Command::EndMatch => DriverCommand::EndMatch,
            // The scores table is chosen with the manialinks of the config, not by modes.
            Command::SetLayer(Layer { name, manialink }) => DriverCommand::SetLayer(DriverLayer {
                name,
                manialink,
                kind: DriverLayerKind::Normal,
            }),
            Command::RemoveLayer(name) => DriverCommand::RemoveLayer(name),
            Command::SetNetState(state) => DriverCommand::SetNetState(state),
            Command::ChatMessage(message) => DriverCommand::ChatMessage(message),
        })
        .collect()
}

fn round_format(format: RoundFormat) -> DriverRoundFormat {
    match format {
        RoundFormat::SingleAttempt(timeout) => DriverRoundFormat::SingleAttempt(match timeout {
            FinishTimeout::Automatic => DriverFinishTimeout::Automatic,
            FinishTimeout::Disabled => DriverFinishTimeout::Disabled,
            FinishTimeout::Milliseconds(ms) => DriverFinishTimeout::Milliseconds(ms),
        }),
        RoundFormat::TimeAttack(time_limit) => DriverRoundFormat::TimeAttack(time_limit),
    }
}

/// Standings without players, as every match starts.
pub(crate) fn empty_standings() -> Standings {
    Standings {
        players: Vec::new(),
        rounds_on_map: 0,
        maps_played: 0,
        info: String::new(),
    }
}

/// The standings as the manialinks and the manager get them.
pub(crate) fn standings(title: &str, standings: &Standings) -> DriverStandings {
    let mut players: Vec<DriverStanding> = standings.players.iter().map(standing).collect();
    // Stable so ties keep the order of the mode. Unranked players come last.
    players.sort_by_key(|player| (player.rank == 0, player.rank));

    DriverStandings {
        title: title.into(),
        info: standings.info.clone(),
        rounds_on_map: standings.rounds_on_map,
        maps_played: standings.maps_played,
        players,
    }
}

fn standing(standing: &Standing) -> DriverStanding {
    DriverStanding {
        account_id: standing.account_id.clone(),
        name: standing.name.clone(),
        rank: standing.rank,
        status: match standing.status {
            Status::Playing => DriverStatus::Playing,
            Status::Finalist => DriverStatus::Finalist,
            Status::LastChance => DriverStatus::LastChance,
            Status::Eliminated => DriverStatus::Eliminated,
            Status::Winner => DriverStatus::Winner,
            Status::Spectator => DriverStatus::Spectator,
        },
        points: standing.points,
        map_points: standing.map_points,
        round_points: standing.round_points,
    }
}

/// The round as the manager records it: the round points of everyone taking part
/// who raced or got points, in the order of the race. Finishers come first by time,
/// then everyone else by the checkpoints they passed.
pub(crate) fn round_result(
    standings: &Standings,
    results: &[RaceResult],
    time_attack: bool,
) -> DriverRoundResult {
    // Time attack rounds count the best race, single attempt rounds the last one.
    let time = |race: &RaceResult| {
        if time_attack {
            race.best_time
        } else {
            race.previous_time
        }
    };

    let mut players: Vec<(&Standing, Option<&RaceResult>)> = standings
        .players
        .iter()
        .map(|player| {
            let race = results.iter().find(|r| r.account_id == player.account_id);
            (player, race)
        })
        .filter(|(player, race)| {
            player.status != Status::Spectator
                && (player.round_points != 0
                    || race.is_some_and(|r| r.started || time(r).is_some()))
        })
        .collect();
    // Stable so ties keep the order of the standings.
    players.sort_by_key(|(_, race)| {
        let time = race.and_then(time);
        (
            time.is_none(),
            time,
            Reverse(race.map_or(0, |r| r.progress)),
        )
    });

    DriverRoundResult {
        results: players
            .into_iter()
            .enumerate()
            .map(|(index, (player, _))| DriverPlayerResult {
                account_id: player.account_id.clone(),
                points: player.round_points,
                position: index as u32 + 1,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(account_id: &str, rank: u32, status: Status, round_points: i32) -> Standing {
        Standing {
            account_id: account_id.into(),
            name: account_id.into(),
            rank,
            status,
            points: round_points,
            map_points: round_points,
            round_points,
        }
    }

    fn race(
        account_id: &str,
        previous: Option<u32>,
        best: Option<u32>,
        progress: u32,
    ) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
            started: true,
            progress,
            previous_time: previous,
            previous_checkpoints: vec![],
            best_time: best,
            best_checkpoints: vec![],
        }
    }

    fn positions(result: &DriverRoundResult) -> Vec<(&str, i32, u32)> {
        result
            .results
            .iter()
            .map(|r| (r.account_id.as_str(), r.points, r.position))
            .collect()
    }

    #[test]
    fn unfinished_races_have_no_time() {
        assert_eq!(race_time(&RoundTime::None), None);
        assert_eq!(race_time(&RoundTime::Time(0)), None);
        assert_eq!(race_time(&RoundTime::Time(45_210)), Some(45_210));
    }

    #[test]
    fn round_formats_keep_their_values() {
        assert_eq!(
            round_format(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                7000
            ))),
            DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Milliseconds(7000))
        );
        assert_eq!(
            round_format(RoundFormat::TimeAttack(60_000)),
            DriverRoundFormat::TimeAttack(60_000)
        );
    }

    #[test]
    fn standings_are_ordered_by_rank() {
        let mut standings = empty_standings();
        standings.info = "1 / 5 rounds".into();
        standings.rounds_on_map = 1;
        standings.players = vec![
            player("unranked", 0, Status::Playing, 0),
            player("second", 2, Status::LastChance, -6),
            player("first", 1, Status::Winner, 10),
        ];

        let standings = super::standings("Rounds", &standings);
        assert_eq!(
            serde_json::to_value(&standings).unwrap(),
            serde_json::json!({
                "title": "Rounds",
                "info": "1 / 5 rounds",
                "rounds_on_map": 1,
                "maps_played": 0,
                "players": [
                    {"account_id": "first", "name": "first", "rank": 1, "status": "winner", "points": 10, "map_points": 10, "round_points": 10},
                    {"account_id": "second", "name": "second", "rank": 2, "status": "last_chance", "points": -6, "map_points": -6, "round_points": -6},
                    {"account_id": "unranked", "name": "unranked", "rank": 0, "status": "playing", "points": 0, "map_points": 0, "round_points": 0},
                ]
            })
        );
    }

    #[test]
    fn round_results_follow_the_race() {
        let mut standings = empty_standings();
        standings.players = vec![
            // Lost nothing without a finish, still behind every finisher.
            player("spared", 1, Status::LastChance, 0),
            player("slow", 2, Status::Playing, -6),
            player("fast", 3, Status::Playing, -2),
            player("far", 4, Status::Playing, -20),
            // Left before starting but lost points.
            player("left", 5, Status::Playing, -20),
            // Did not race and got nothing.
            player("idle", 6, Status::Playing, 0),
            player("spectator", 0, Status::Spectator, 0),
        ];
        let results = [
            race("slow", Some(31_000), Some(31_000), 3),
            race("spared", None, None, 1),
            race("far", None, None, 2),
            race("fast", Some(30_000), Some(30_000), 3),
            // Not in the standings, e.g. a late player.
            race("late", Some(29_000), Some(29_000), 3),
            // Raced without taking part.
            race("spectator", Some(28_000), Some(28_000), 3),
        ];

        assert_eq!(
            positions(&round_result(&standings, &results, false)),
            [
                ("fast", -2, 1),
                ("slow", -6, 2),
                ("far", -20, 3),
                ("spared", 0, 4),
                ("left", -20, 5),
            ]
        );
    }

    #[test]
    fn time_attack_rounds_count_the_best_race() {
        let mut standings = empty_standings();
        standings.players = vec![
            player("a", 2, Status::Playing, 1),
            player("b", 1, Status::Playing, 2),
        ];
        let results = [
            race("a", Some(42_000), Some(42_000), 3),
            race("b", Some(45_000), Some(41_000), 3),
        ];

        assert_eq!(
            positions(&round_result(&standings, &results, true)),
            [("b", 2, 1), ("a", 1, 2)]
        );
        assert_eq!(
            positions(&round_result(&standings, &results, false)),
            [("a", 1, 1), ("b", 2, 2)]
        );
    }
}
