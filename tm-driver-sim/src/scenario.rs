//! Scenarios: worlds written by hand, e.g. for a rule of a mode or a match that
//! went wrong on a server. A mode keeps its scenarios with its tests and runs them
//! with [`Simulator::scenario`](crate::Simulator::scenario).
//!
//! ```text
//! # Two players finish in the same time, the account decides.
//! settings {"rounds_without_elimination": 0}
//! maps m1:10/10/10
//! players a b c=Cid
//! round a 30.1 | b 30.1 | c dnf 1
//! ```
//!
//! One instruction per line, `#` starts a comment:
//!
//! - `settings <json>`: the settings of the mode, `{}` by default.
//! - `maps <uid>[:<sector>/<sector>/...] ...`: the map list of the server, with
//!   the seconds the author needs for every sector, the last one ends at the
//!   finish. `m1:10/10/10` by default. The first map runs when the driver loads.
//! - `players <id>[=<name>] ...`: players on the server when the driver loads,
//!   `watchers` the same for players who only watch, `later` for players who
//!   join with `join`.
//! - `matches <n>`: matches that end before the simulation does, 1 by default.
//! - `round [forced] <id> <race> ... | <id> <race> ...`: a round. A race is a
//!   finish time in seconds (`30.1`, `1:02.5`), the race times at every
//!   checkpoint up to the finish (`10/20.5/30.1`), `dnf <checkpoints>` for giving
//!   up after passing that many checkpoints, `dnf <times>` after passing them at
//!   those times, or `idle`. Rounds with restarts take several races per player.
//!   Players who could race but are missing do not move. `forced` ends the round
//!   once the races are over, so it does not count.
//!
//! Before a round:
//!
//! - `join <id> ...` and `leave <id> ...`.
//! - `playlist <uid> ...`: a new playlist for the driver. Before the first round
//!   it is the playlist the driver loads with.
//! - `repeat-holds on|off`: every hold reaches the driver twice from now on.
//! - `restart-bridge`: the driver resumes by replaying its inputs.
//! - `restart-script`: the modescript restarts, which starts a new match.
//!
//! Commands of the driver steer the server like a real one, so who races and how
//! long the match lasts follow the mode. The simulation ends after the last round
//! of the scenario, or earlier when the matches end.

use std::collections::BTreeMap;

use crate::{
    Faults, Moment, PlayerSpec, RacePlan, Races, ScriptedRound, World,
    world::{Action, MapSpec},
};

pub fn parse(text: &str) -> Result<World, String> {
    let mut settings = "{}".to_string();
    let mut maps = Vec::new();
    let mut playlist = Vec::new();
    let mut players: Vec<PlayerSpec> = Vec::new();
    let mut matches = 1;
    let mut rounds: Vec<ScriptedRound> = Vec::new();
    let mut before = Vec::new();

    for (number, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let error = |message: String| format!("Line {}: {message}", number + 1);
        let (instruction, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        let words = || rest.split_whitespace();

        match instruction {
            "settings" => settings = rest.to_string(),
            "maps" => {
                maps = words()
                    .map(|spec| map(spec).map_err(&error))
                    .collect::<Result<_, _>>()?;
            }
            "players" | "watchers" | "later" => {
                for spec in words() {
                    let (account_id, name) = spec.split_once('=').unwrap_or((spec, spec));
                    if players.iter().any(|p| p.account_id == account_id) {
                        return Err(error(format!("{account_id} is there twice")));
                    }
                    players.push(PlayerSpec {
                        account_id: account_id.into(),
                        name: name.into(),
                        pace: 1_000,
                        spread: 0,
                        give_up: 0,
                        // Set once the round they join is known.
                        joins: (instruction == "later").then_some(Moment {
                            round: 0,
                            during: false,
                        }),
                        leaves: None,
                        watches: instruction == "watchers",
                    });
                }
            }
            "matches" => {
                matches = rest
                    .parse()
                    .map_err(|_| error(format!("{rest} is no number")))?;
            }
            "join" | "leave" => {
                for account_id in words() {
                    if !players.iter().any(|p| p.account_id == account_id) {
                        return Err(error(format!("Nobody is called {account_id}")));
                    }
                    before.push(if instruction == "join" {
                        Action::Join(account_id.into())
                    } else {
                        Action::Leave(account_id.into())
                    });
                }
            }
            "playlist" => {
                let uids: Vec<String> = words().map(String::from).collect();
                if rounds.is_empty() && before.is_empty() {
                    playlist = uids;
                } else {
                    before.push(Action::Playlist(uids));
                }
            }
            "repeat-holds" => match rest {
                "on" => before.push(Action::RepeatHolds(true)),
                "off" => before.push(Action::RepeatHolds(false)),
                _ => return Err(error("repeat-holds is on or off".into())),
            },
            "restart-bridge" => before.push(Action::RestartBridge),
            "restart-script" => before.push(Action::RestartScript),
            "round" => {
                let (forced, races) = match rest.strip_prefix("forced") {
                    Some(races) => (true, races),
                    None => (false, rest),
                };
                let mut round = ScriptedRound {
                    before: std::mem::take(&mut before),
                    forced,
                    races: BTreeMap::new(),
                };
                for segment in races.split('|').filter(|s| !s.trim().is_empty()) {
                    let mut tokens = segment.split_whitespace();
                    let account_id = tokens.next().expect("Segments are not empty");
                    if !players.iter().any(|p| p.account_id == account_id) {
                        return Err(error(format!("Nobody is called {account_id}")));
                    }
                    let mut plans = Vec::new();
                    while let Some(token) = tokens.next() {
                        plans.push(race(token, &mut tokens).map_err(&error)?);
                    }
                    round.races.insert(account_id.into(), plans);
                }
                rounds.push(round);
            }
            _ => return Err(error(format!("Unknown instruction {instruction}"))),
        }
    }
    if !before.is_empty() {
        return Err("Instructions after the last round do nothing".into());
    }

    // The header of the report tells when players come and go.
    for player in &mut players {
        let first = |wanted: Action| {
            rounds
                .iter()
                .position(|round| round.before.contains(&wanted))
                .map(|index| Moment {
                    round: index as u32 + 1,
                    during: false,
                })
        };
        if player.joins.is_some() {
            player.joins = Some(
                first(Action::Join(player.account_id.clone()))
                    .ok_or_else(|| format!("{} never joins", player.account_id))?,
            );
        }
        player.leaves = first(Action::Leave(player.account_id.clone()));
    }
    if maps.is_empty() {
        maps.push(map("m1").expect("A valid map"));
    }

    let max_rounds = rounds.len() as u32 + 1;
    Ok(World {
        settings,
        maps,
        playlist,
        players,
        races: Races::Scripted(rounds),
        faults: Faults::default(),
        matches,
        max_rounds,
    })
}

/// `m1` or `m1:10/10/10.5` with the seconds of every sector.
fn map(spec: &str) -> Result<MapSpec, String> {
    let (uid, sectors) = spec.split_once(':').unwrap_or((spec, "10/10/10"));
    let sectors = sectors
        .split('/')
        .map(seconds)
        .collect::<Result<Vec<u32>, _>>()?;
    if sectors.contains(&0) {
        return Err(format!("A sector of {uid} takes no time"));
    }
    Ok(MapSpec {
        uid: uid.into(),
        sectors,
    })
}

fn race<'a>(token: &str, tokens: &mut impl Iterator<Item = &'a str>) -> Result<RacePlan, String> {
    Ok(match token {
        "idle" => RacePlan::Idle,
        "dnf" => {
            let passed = tokens.next().ok_or("dnf needs the checkpoints passed")?;
            if passed.contains(['.', '/', ':']) {
                RacePlan::GiveUp(times(passed)?)
            } else {
                RacePlan::GiveUpAfter(
                    passed
                        .parse()
                        .map_err(|_| format!("{passed} is no number of checkpoints"))?,
                )
            }
        }
        token if token.contains('/') => RacePlan::Finish(times(token)?),
        token => RacePlan::FinishIn(seconds(token)?),
    })
}

/// Race times at checkpoints in order, e.g. `10/20.5/30.1`.
fn times(token: &str) -> Result<Vec<u32>, String> {
    let times = token
        .split('/')
        .map(seconds)
        .collect::<Result<Vec<u32>, _>>()?;
    if times.windows(2).any(|pair| pair[0] >= pair[1]) || times.first() == Some(&0) {
        return Err(format!("The race times of {token} have to grow"));
    }
    Ok(times)
}

/// Seconds like `30`, `30.1` or `1:02.345` in milliseconds.
fn seconds(token: &str) -> Result<u32, String> {
    let invalid = || format!("{token} is no time");
    let (minutes, rest) = match token.split_once(':') {
        Some((minutes, rest)) => (minutes.parse::<u32>().map_err(|_| invalid())?, rest),
        None => (0, token),
    };
    let (seconds, fraction) = rest.split_once('.').unwrap_or((rest, ""));
    if fraction.len() > 3 || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(invalid());
    }
    let seconds: u32 = seconds.parse().map_err(|_| invalid())?;
    let milliseconds: u32 = format!("{fraction:0<3}").parse().map_err(|_| invalid())?;
    Ok(minutes * 60_000 + seconds * 1_000 + milliseconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_in_seconds() {
        assert_eq!(seconds("30"), Ok(30_000));
        assert_eq!(seconds("30.1"), Ok(30_100));
        assert_eq!(seconds("1:02.345"), Ok(62_345));
        assert!(seconds("30.1234").is_err());
        assert!(seconds("fast").is_err());
    }

    #[test]
    fn a_scenario_becomes_a_world() {
        let world = parse(
            "settings {\"lives\": 2}
             maps m1:10/20 m2
             players a b=Bea
             later c
             round a 30 | b dnf 1
             join c
             repeat-holds on
             round forced a 10/29.5 | b idle | c 31 30.5",
        )
        .unwrap();

        assert_eq!(world.maps[0].sectors, [10_000, 20_000]);
        assert_eq!(world.maps[1].sectors, [10_000, 10_000, 10_000]);
        assert_eq!(world.players[1].name, "Bea");
        assert_eq!(
            world.players[2].joins,
            Some(Moment {
                round: 2,
                during: false
            })
        );
        let Races::Scripted(rounds) = &world.races else {
            panic!("A scenario is scripted");
        };
        assert_eq!(rounds[0].races["b"], [RacePlan::GiveUpAfter(1)]);
        assert!(rounds[1].forced);
        assert_eq!(
            rounds[1].before,
            [Action::Join("c".into()), Action::RepeatHolds(true)]
        );
        assert_eq!(
            rounds[1].races["c"],
            [RacePlan::FinishIn(31_000), RacePlan::FinishIn(30_500)]
        );
        assert_eq!(
            rounds[1].races["a"],
            [RacePlan::Finish(vec![10_000, 29_500])]
        );
    }

    #[test]
    fn mistakes_are_reported_with_their_line() {
        assert_eq!(
            parse("players a\nround x 30").err().as_deref(),
            Some("Line 2: Nobody is called x")
        );
        assert!(parse("mode knockout").is_err());
        assert!(parse("players a\nround a dnf").is_err());
        assert!(parse("later a\nround").is_err());
    }
}
