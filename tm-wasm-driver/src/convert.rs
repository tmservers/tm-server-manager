use std::collections::BTreeMap;

use tm_server_types::{
    event::{DriverPlayerResult, DriverRoundResult},
    method::{
        DriverCanSpawn, DriverCommand, DriverEntry, DriverFinishTimeout, DriverLabelTone,
        DriverLaps, DriverRaceRules, DriverRound, DriverRoundFormat, DriverStandings, DriverStatus,
        DriverUser, DriverUsers, DriverWarmUp,
    },
};

use crate::bindings::tmservers::driver::types::{
    CanSpawn, Command, Entry, FinishTimeout, Label, Laps, RaceRules, RoundFormat, Standings,
    Status, Tone, User, WarmUp,
};

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
                Command::SetRaceRules(RaceRules { laps, give_up }) => {
                    DriverCommand::SetRaceRules(DriverRaceRules {
                        laps: match laps {
                            Laps::Map => DriverLaps::Map,
                            Laps::Count(laps) => DriverLaps::Count(laps),
                            Laps::Infinite => DriverLaps::Infinite,
                        },
                        give_up,
                    })
                }
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
        entries: Vec::new(),
        round: Vec::new(),
        rounds_on_map: 0,
        rounds_played: 0,
        maps_played: 0,
        info: String::new(),
    }
}

/// The standings as the manialinks and the manager get them.
pub(crate) fn standings(
    title: &str,
    map_name: &str,
    map_count: u32,
    paused: bool,
    standings: &Standings,
    users: &BTreeMap<String, User>,
) -> DriverStandings {
    let mut entries: Vec<DriverEntry> = standings.entries.iter().map(entry).collect();
    // Players without a position follow by name, spectators last. That order is
    // only for display, so the host makes it instead of the modes.
    entries.sort_by_cached_key(|entry| {
        (
            entry.position == 0,
            entry.position,
            entry.status == DriverStatus::Spectator,
            name(users, &entry.account_id).to_lowercase(),
            entry.account_id.clone(),
        )
    });

    DriverStandings {
        title: title.into(),
        info: standings.info.clone(),
        rounds_on_map: standings.rounds_on_map,
        maps_played: standings.maps_played,
        map_name: map_name.into(),
        map_count,
        paused,
        entries,
    }
}

/// The name of a player for display, the account while the host does not know it.
fn name<'a>(users: &'a BTreeMap<String, User>, account_id: &'a str) -> &'a str {
    users
        .get(account_id)
        .map(|user| user.name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(account_id)
}

/// The round as the manialinks get it.
pub(crate) fn round(number: u32, live: bool, entries: &[Entry]) -> DriverRound {
    DriverRound {
        number,
        live,
        entries: ordered(entries),
    }
}

/// The players the host knows as the manialinks get them.
pub(crate) fn users(users: &BTreeMap<String, User>) -> DriverUsers {
    DriverUsers {
        users: users
            .values()
            .map(|user| DriverUser {
                account_id: user.account_id.clone(),
                name: user.name.clone(),
                club_tag: user.club_tag.clone(),
                zone: user.zone.clone(),
            })
            .collect(),
    }
}

/// The round as the manager records it: the round points of everyone the mode
/// placed in the round, in the order of their places.
pub(crate) fn round_result(entries: &[Entry]) -> DriverRoundResult {
    DriverRoundResult {
        results: ordered(entries)
            .into_iter()
            .filter(|entry| entry.position > 0)
            .map(|entry| DriverPlayerResult {
                account_id: entry.account_id,
                points: entry.score,
                position: entry.position,
            })
            .collect(),
    }
}

/// Ordered by position. Stable so ties keep the order of the mode, unplaced players last.
fn ordered(entries: &[Entry]) -> Vec<DriverEntry> {
    let mut entries: Vec<DriverEntry> = entries.iter().map(entry).collect();
    entries.sort_by_key(|entry| (entry.position == 0, entry.position));
    entries
}

fn entry(entry: &Entry) -> DriverEntry {
    // The manialinks parse the json into structs, so missing values have defaults.
    let (label, label_tone) = match &entry.label {
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

    DriverEntry {
        account_id: entry.account_id.clone(),
        position: entry.position,
        score: entry.score,
        time: entry
            .time
            .map_or(-1, |time| i32::try_from(time).unwrap_or(i32::MAX)),
        status: match entry.status {
            Status::Playing => DriverStatus::Playing,
            Status::Eliminated => DriverStatus::Eliminated,
            Status::Spectator => DriverStatus::Spectator,
        },
        label,
        label_tone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(account_id: &str, position: u32, status: Status, score: i32) -> Entry {
        Entry {
            account_id: account_id.into(),
            position,
            score,
            time: None,
            status,
            label: None,
        }
    }

    fn user(account_id: &str, name: &str) -> User {
        User {
            account_id: account_id.into(),
            name: name.into(),
            club_tag: String::new(),
            zone: String::new(),
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
    fn race_rules_keep_their_values() {
        assert_eq!(
            commands(
                vec![Command::SetRaceRules(RaceRules {
                    laps: Laps::Count(50),
                    give_up: false,
                })],
                &[]
            ),
            [DriverCommand::SetRaceRules(DriverRaceRules {
                laps: DriverLaps::Count(50),
                give_up: false,
            })]
        );
    }

    #[test]
    fn standings_are_ordered_by_position() {
        let mut standings = empty_standings();
        standings.info = "1 / 5 rounds".into();
        standings.rounds_on_map = 1;
        let mut last_chance = player("second", 2, Status::Playing, 0);
        last_chance.label = Some(Label {
            text: "Last Chance".into(),
            tone: Tone::Negative,
        });
        let mut first = player("first", 1, Status::Playing, 10);
        first.time = Some(30_000);
        standings.entries = vec![
            player("unranked", 0, Status::Playing, 0),
            last_chance,
            first,
            player("out", 3, Status::Eliminated, 0),
        ];

        let standings = super::standings("Rounds", "Map", 2, false, &standings, &BTreeMap::new());
        assert_eq!(
            serde_json::to_value(&standings).unwrap(),
            serde_json::json!({
                "title": "Rounds",
                "info": "1 / 5 rounds",
                "rounds_on_map": 1,
                "maps_played": 0,
                "map_name": "Map",
                "map_count": 2,
                "paused": false,
                "entries": [
                    {"account_id": "first", "position": 1, "score": 10, "time": 30000, "status": "playing", "label": "", "label_tone": "neutral"},
                    {"account_id": "second", "position": 2, "score": 0, "time": -1, "status": "playing", "label": "Last Chance", "label_tone": "negative"},
                    {"account_id": "out", "position": 3, "score": 0, "time": -1, "status": "eliminated", "label": "", "label_tone": "neutral"},
                    {"account_id": "unranked", "position": 0, "score": 0, "time": -1, "status": "playing", "label": "", "label_tone": "neutral"},
                ]
            })
        );
    }

    #[test]
    fn players_without_a_position_follow_by_name() {
        let users = BTreeMap::from([
            ("y".to_string(), user("y", "bob")),
            ("z".to_string(), user("z", "Alice")),
        ]);
        let mut standings = empty_standings();
        standings.entries = vec![
            player("s", 0, Status::Spectator, 0),
            player("y", 0, Status::Playing, 0),
            player("x", 0, Status::Playing, 0),
            player("first", 1, Status::Playing, 0),
            player("z", 0, Status::Playing, 0),
        ];

        let order: Vec<String> = super::standings("Rounds", "Map", 0, false, &standings, &users)
            .entries
            .into_iter()
            .map(|entry| entry.account_id)
            .collect();
        // Whatever the case. The account stands in for an unknown name.
        assert_eq!(order, ["first", "z", "y", "x", "s"]);
    }

    #[test]
    fn round_results_follow_the_places_of_the_mode() {
        let round = [
            player("second", 2, Status::Playing, -6),
            // Not placed by the mode.
            player("idle", 0, Status::Playing, 0),
            player("first", 1, Status::Playing, -2),
            // Took part without points.
            player("third", 3, Status::Eliminated, 0),
        ];

        assert_eq!(
            positions(&round_result(&round)),
            [("first", -2, 1), ("second", -6, 2), ("third", 0, 3)]
        );
        assert_eq!(
            super::round(4, false, &round).entries[0].account_id,
            "first"
        );
    }

    #[test]
    fn users_are_ordered_by_account() {
        let users = BTreeMap::from([
            ("b".to_string(), user("b", "$f00B")),
            ("a".to_string(), user("a", "A")),
        ]);
        assert_eq!(
            serde_json::to_value(super::users(&users)).unwrap(),
            serde_json::json!({"users": [
                {"account_id": "a", "name": "A", "club_tag": "", "zone": ""},
                {"account_id": "b", "name": "$f00B", "club_tag": "", "zone": ""},
            ]})
        );
    }
}
