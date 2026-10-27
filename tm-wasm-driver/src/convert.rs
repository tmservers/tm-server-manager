use std::collections::BTreeMap;

use tm_server_types::{
    base::{Player, RoundTime},
    event::{DriverPlayerResult, DriverRoundResult, Scores},
    method::{
        DriverCanSpawn, DriverCommand, DriverFinishTimeout, DriverLabelTone, DriverLayer,
        DriverLayerKind, DriverRoundFormat, DriverStanding, DriverStandings, DriverStatus,
        DriverWarmUp,
    },
};

use crate::bindings::tmservers::driver::types::{
    CanSpawn, Command, FinishTimeout, Label, Layer, RaceResult, RoundFormat, Standing, Standings,
    Status, Tone, WarmUp,
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

/// The commands of a mode for the server. Maps are given by their place in the playlist.
pub(crate) fn commands(commands: Vec<Command>, playlist: &[String]) -> Vec<DriverCommand> {
    commands
        .into_iter()
        .filter_map(|command| {
            Some(match command {
                Command::SetRoundFormat(format) => {
                    DriverCommand::SetRoundFormat(round_format(format))
                }
                Command::SetWarmUp(WarmUp {
                    rounds,
                    duration_ms,
                    timeout_ms,
                }) => DriverCommand::SetWarmUp(DriverWarmUp {
                    rounds,
                    duration_ms,
                    timeout_ms,
                }),
                Command::SetCanSpawn(CanSpawn {
                    account_id,
                    can_spawn,
                }) => DriverCommand::SetCanSpawn(DriverCanSpawn {
                    account_id,
                    can_spawn,
                }),
                Command::EndMap => DriverCommand::EndMap,
                Command::EndMatch => DriverCommand::EndMatch,
                Command::SetNextMap(index) => match playlist.get(index as usize) {
                    Some(uid) => DriverCommand::SetNextMap(uid.clone()),
                    None => {
                        tracing::warn!(
                            "The mode chose map {index} of a playlist with {} maps.",
                            playlist.len()
                        );
                        return None;
                    }
                },
                // The scores table is chosen with the manialinks of the config, not by modes.
                Command::SetLayer(Layer { name, manialink }) => {
                    DriverCommand::SetLayer(DriverLayer {
                        name,
                        manialink,
                        kind: DriverLayerKind::Normal,
                    })
                }
                Command::RemoveLayer(name) => DriverCommand::RemoveLayer(name),
                Command::SetNetState(state) => DriverCommand::SetNetState(state),
                Command::ChatMessage(message) => DriverCommand::ChatMessage(message),
            })
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
        map_name: String::new(),
        map_count: 0,
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
        map_name: standings.map_name.clone(),
        map_count: standings.map_count,
        players,
    }
}

fn standing(standing: &Standing) -> DriverStanding {
    // The manialinks parse the json into structs, so a missing label is empty.
    let (label, label_tone) = match &standing.label {
        Some(Label { text, tone }) => (
            text.clone(),
            match tone {
                Tone::Neutral => DriverLabelTone::Neutral,
                Tone::Positive => DriverLabelTone::Positive,
                Tone::Negative => DriverLabelTone::Negative,
            },
        ),
        None => (String::new(), DriverLabelTone::Neutral),
    };

    DriverStanding {
        account_id: standing.account_id.clone(),
        name: standing.name.clone(),
        rank: standing.rank,
        status: match standing.status {
            Status::Playing => DriverStatus::Playing,
            Status::Eliminated => DriverStatus::Eliminated,
            Status::Spectator => DriverStatus::Spectator,
        },
        label,
        label_tone,
        points: standing.points,
        map_points: standing.map_points,
        round_points: standing.round_points,
    }
}

/// The round as the manager records it: the round points of everyone the mode
/// placed in the round, in the order of their places.
pub(crate) fn round_result(standings: &Standings) -> DriverRoundResult {
    let mut players: Vec<&Standing> = standings
        .players
        .iter()
        .filter(|player| player.round_position > 0)
        .collect();
    // Stable so shared places keep the order of the standings.
    players.sort_by_key(|player| player.round_position);

    DriverRoundResult {
        results: players
            .into_iter()
            .map(|player| DriverPlayerResult {
                account_id: player.account_id.clone(),
                points: player.round_points,
                position: player.round_position,
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
            label: None,
            points: round_points,
            map_points: round_points,
            round_points,
            round_position: 0,
        }
    }

    fn placed(account_id: &str, round_points: i32, round_position: u32) -> Standing {
        Standing {
            round_position,
            ..player(account_id, 0, Status::Playing, round_points)
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
    fn maps_are_chosen_from_the_playlist() {
        let playlist = ["first".to_string(), "second".to_string()];
        assert_eq!(
            commands(
                vec![
                    Command::SetNextMap(1),
                    Command::SetNextMap(2),
                    Command::EndMap
                ],
                &playlist
            ),
            [
                DriverCommand::SetNextMap("second".into()),
                DriverCommand::EndMap
            ]
        );
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
        let mut last_chance = player("second", 2, Status::Playing, -6);
        last_chance.label = Some(Label {
            text: "Last Chance".into(),
            tone: Tone::Negative,
        });
        standings.players = vec![
            player("unranked", 0, Status::Playing, 0),
            last_chance,
            player("first", 1, Status::Playing, 10),
            player("out", 3, Status::Eliminated, 0),
        ];

        let standings = super::standings("Rounds", &standings);
        assert_eq!(
            serde_json::to_value(&standings).unwrap(),
            serde_json::json!({
                "title": "Rounds",
                "info": "1 / 5 rounds",
                "rounds_on_map": 1,
                "maps_played": 0,
                "map_name": "",
                "map_count": 0,
                "players": [
                    {"account_id": "first", "name": "first", "rank": 1, "status": "playing", "label": "", "label_tone": "neutral", "points": 10, "map_points": 10, "round_points": 10},
                    {"account_id": "second", "name": "second", "rank": 2, "status": "playing", "label": "Last Chance", "label_tone": "negative", "points": -6, "map_points": -6, "round_points": -6},
                    {"account_id": "out", "name": "out", "rank": 3, "status": "eliminated", "label": "", "label_tone": "neutral", "points": 0, "map_points": 0, "round_points": 0},
                    {"account_id": "unranked", "name": "unranked", "rank": 0, "status": "playing", "label": "", "label_tone": "neutral", "points": 0, "map_points": 0, "round_points": 0},
                ]
            })
        );
    }

    #[test]
    fn round_results_follow_the_places_of_the_mode() {
        let mut standings = empty_standings();
        standings.players = vec![
            placed("second", -6, 2),
            // Did not take part in the round.
            placed("idle", 0, 0),
            placed("first", -2, 1),
            // Took part without points.
            placed("third", 0, 3),
        ];

        assert_eq!(
            positions(&round_result(&standings)),
            [("first", -2, 1), ("second", -6, 2), ("third", 0, 3)]
        );
    }
}
