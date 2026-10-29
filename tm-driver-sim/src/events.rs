//! Inputs of a driver written directly: the callbacks of the server and logs of
//! them, e.g. for a callback order or fault the fake server does not produce, or a
//! match that went wrong.
//!
//! A log is open: unlike a simulation, it does not follow the commands of the
//! driver. Who races and when maps end are written down. The checks still run on
//! it, so a log that no longer fits a mode fails instead of testing a match the
//! driver never asked for.
//!
//! ```ignore
//! let mut log = InputLog::new(r#"{"rounds_without_elimination": 0}"#);
//! log.players(&["a", "b", "c"]).match_start().map_start("m1");
//! log.round(&[
//!     ("a", Race::finish(&[10_000, 30_000])),
//!     ("b", Race::finish(&[10_000, 31_000])),
//!     ("c", Race::dnf(&[11_000])),
//! ]);
//! let outcome = simulator().log("a knockout", &log);
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};
use tm_server_types::{
    base::{Map, PlayerInfo},
    event::{
        DriverHold, DriverSection, EndMatch, Event, Pause, PlayerConnect, PlayerDisconnect,
        StartLine, StartMap, StartMatch, StartRound, WayPoint,
    },
    method::{DriverCommand, DriverStatus, DriverWarmUp},
};
use tm_wasm_driver::{Driver, DriverInput, DriverReply};

/// The session of the modescript in the holds this module builds.
pub const SESSION: i32 = 7;

/// The settings of a match and every input its driver got, e.g. a match that went
/// wrong, saved as json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputLog {
    pub settings: String,
    pub inputs: Vec<DriverInput>,
    /// The inputs whose reply never reached the server, counted from 0, e.g. when
    /// the bridge went down before it went out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lost: Vec<usize>,
    /// The session of the holds the builder adds.
    #[serde(skip)]
    pub(crate) session: i32,
}

/// A race of a log: the race times at the checkpoints it passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Race {
    pub checkpoints: Vec<u32>,
    pub finished: bool,
}

impl Race {
    /// A race to the finish with these race times at the checkpoints, the finish last.
    pub fn finish(checkpoints: &[u32]) -> Self {
        Self {
            checkpoints: checkpoints.to_vec(),
            finished: true,
        }
    }

    /// A race given up after passing checkpoints at these race times.
    pub fn dnf(checkpoints: &[u32]) -> Self {
        Self {
            checkpoints: checkpoints.to_vec(),
            finished: false,
        }
    }
}

impl InputLog {
    pub fn new(settings: impl Into<String>) -> Self {
        Self {
            settings: settings.into(),
            inputs: Vec::new(),
            lost: Vec::new(),
            session: SESSION,
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("{} could not be read: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{} is no log: {e}", path.display()))
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let json = serde_json::to_string_pretty(self).expect("Logs are json");
        std::fs::write(path, json)
            .map_err(|e| format!("{} could not be written: {e}", path.display()))
    }

    pub fn push(&mut self, input: DriverInput) -> &mut Self {
        self.inputs.push(input);
        self
    }

    pub fn event(&mut self, event: Event) -> &mut Self {
        self.push(DriverInput::Event(event))
    }

    /// The players on the server when the driver loads, named after their accounts.
    pub fn players(&mut self, accounts: &[&str]) -> &mut Self {
        let mut players = vec![server()];
        players.extend(accounts.iter().map(|account| player(account, account)));
        self.push(DriverInput::Players(players))
    }

    pub fn playlist(&mut self, uids: &[&str]) -> &mut Self {
        self.push(DriverInput::Playlist(
            uids.iter().map(|uid| uid.to_string()).collect(),
        ))
    }

    pub fn join(&mut self, account_id: &str) -> &mut Self {
        self.event(Event::PlayerConnect(PlayerConnect {
            account_id: account_id.into(),
            is_spectator: false,
        }));
        self.event(player_info(account_id, account_id))
    }

    pub fn leave(&mut self, account_id: &str) -> &mut Self {
        self.event(Event::PlayerDisconnect(PlayerDisconnect {
            account_id: account_id.into(),
            disconnect_reason: String::new(),
        }))
    }

    pub fn match_start(&mut self) -> &mut Self {
        let count = self.count(|event| matches!(event, Event::StartMatchStart(_))) + 1;
        self.event(Event::StartMatchStart(StartMatch { count, time: 0 }))
    }

    pub fn match_end(&mut self) -> &mut Self {
        let count = self.count(|event| matches!(event, Event::StartMatchStart(_)));
        self.event(Event::EndMatchStart(EndMatch { count, time: 0 }))
    }

    /// A map starts and waits for the driver.
    pub fn map_start(&mut self, uid: &str) -> &mut Self {
        self.event(map_start(uid));
        self.hold(DriverSection::MapStart, false)
    }

    /// The map ends and waits for the driver.
    pub fn map_end(&mut self) -> &mut Self {
        self.hold(DriverSection::MapEnd, false)
    }

    /// A round in which every player starts at once and drives their races. A player
    /// named several times drives several races one after the other, like in time
    /// attack rounds. Ends with its hold.
    pub fn round(&mut self, races: &[(&str, Race)]) -> &mut Self {
        self.round_start();
        self.races(races);
        self.hold(DriverSection::RoundEnd, false)
    }

    /// A round an admin forces to end once the races are over, so it does not count.
    pub fn forced_round(&mut self, races: &[(&str, Race)]) -> &mut Self {
        self.round_start();
        self.races(races);
        self.hold(DriverSection::RoundEnd, true)
    }

    pub fn round_start(&mut self) -> &mut Self {
        let count = self.count(|event| matches!(event, Event::StartRoundStart(_))) + 1;
        self.event(round_start(count))
    }

    /// The start lines and waypoints of these races in the order of their time.
    /// Every player starts at 0, further races of a player 1.5 seconds after the
    /// last one ended.
    pub fn races(&mut self, races: &[(&str, Race)]) -> &mut Self {
        let mut callbacks: Vec<(u32, usize, Event)> = Vec::new();
        let mut starts: std::collections::BTreeMap<&str, u32> = Default::default();
        for (account_id, race) in races {
            let start = starts.entry(account_id).or_default();
            callbacks.push((*start, callbacks.len(), start_line(account_id)));
            for (index, &time) in race.checkpoints.iter().enumerate() {
                let so_far = &race.checkpoints[..=index];
                let finished = race.finished && index + 1 == race.checkpoints.len();
                callbacks.push((
                    *start + time,
                    callbacks.len(),
                    way_point_at(account_id, so_far, finished),
                ));
            }
            *start += race.checkpoints.last().copied().unwrap_or(0) + 1_500;
        }
        callbacks.sort_by_key(|(time, order, _)| (*time, *order));
        for (_, _, event) in callbacks {
            self.event(event);
        }
        self
    }

    /// The reply to the last input never reached the server, e.g. because the
    /// bridge went down.
    pub fn lose_reply(&mut self) -> &mut Self {
        if let Some(last) = self.inputs.len().checked_sub(1) {
            self.lost.push(last);
        }
        self
    }

    /// The driver resumed and sends everything it set on the server again.
    pub fn resumed(&mut self) -> &mut Self {
        self.push(DriverInput::Resumed)
    }

    /// New settings for the match, e.g. from an admin.
    pub fn settings(&mut self, settings: &str) -> &mut Self {
        self.push(DriverInput::Settings(settings.into()))
    }

    /// The modescript says whether it is paused, e.g. answering an admin.
    pub fn pause(&mut self, active: bool) -> &mut Self {
        self.event(Event::Pause(Pause {
            available: true,
            active,
        }))
    }

    /// The reply to the last hold reached the server, as the bridge tells it.
    pub fn delivered(&mut self) -> &mut Self {
        self.push(DriverInput::Delivered)
    }

    /// The last hold reaches the driver again, like when the modescript asks again.
    pub fn hold_again(&mut self) -> &mut Self {
        let hold = self.last_hold().cloned().expect("A hold to repeat");
        self.event(Event::DriverHold(hold))
    }

    /// The modescript restarts, which starts a new match and counts holds from 1
    /// again.
    pub fn restart_script(&mut self) -> &mut Self {
        self.session += 1;
        self.match_start()
    }

    fn hold(&mut self, section: DriverSection, forced: bool) -> &mut Self {
        let session = self.session;
        let seq = match self.last_hold() {
            Some(hold) if hold.session == session => hold.seq + 1,
            _ => 1,
        };
        self.event(Event::DriverHold(DriverHold {
            session,
            seq,
            section,
            forced,
        }))
    }

    fn last_hold(&self) -> Option<&DriverHold> {
        self.inputs.iter().rev().find_map(|input| match input {
            DriverInput::Event(Event::DriverHold(hold)) => Some(hold),
            _ => None,
        })
    }

    fn count(&self, wanted: impl Fn(&Event) -> bool) -> u32 {
        self.inputs
            .iter()
            .filter(|input| matches!(input, DriverInput::Event(event) if wanted(event)))
            .count() as u32
    }
}

pub fn hold(seq: u32, section: DriverSection) -> Event {
    Event::DriverHold(DriverHold {
        session: SESSION,
        seq,
        section,
        forced: false,
    })
}

pub fn forced_round_end(seq: u32) -> Event {
    Event::DriverHold(DriverHold {
        session: SESSION,
        seq,
        section: DriverSection::RoundEnd,
        forced: true,
    })
}

pub fn match_start() -> Event {
    Event::StartMatchStart(StartMatch { count: 1, time: 0 })
}

/// The standard callback before the map start hold, the map is named after its uid.
pub fn map_start(uid: &str) -> Event {
    Event::StartMapStart(StartMap {
        count: 1,
        valid: 1,
        restarted: false,
        time: 0,
        map: Map {
            uid: uid.into(),
            name: format!("Map {uid}"),
            filename: format!("{uid}.Map.Gbx"),
            author_account_id: String::new(),
            author_nickname: String::new(),
            environment: "Stadium".into(),
            mood: String::new(),
            bronze_time: 0,
            silver_time: 0,
            gold_time: 0,
            author_time: 30_000,
            copperprice: 0,
            lap_race: false,
            number_laps: 0,
            map_type: r"TrackMania\TM_Race".into(),
            map_style: String::new(),
        },
    })
}

pub fn round_start(count: u32) -> Event {
    Event::StartRoundStart(StartRound { count, time: 0 })
}

pub fn start_line(account_id: &str) -> Event {
    Event::StartLine(StartLine {
        account_id: account_id.into(),
        time: 0,
    })
}

/// A player passed a checkpoint of the race, counting from 0, ten seconds after the
/// one before. Like the server, it carries the race times of every checkpoint so far.
pub fn way_point(account_id: &str, checkpoint_in_race: u32) -> Event {
    let checkpoints: Vec<u32> = (1..=checkpoint_in_race + 1)
        .map(|checkpoint| checkpoint * 10_000)
        .collect();
    way_point_at(account_id, &checkpoints, false)
}

/// A player passed the last of these checkpoints, or finished there.
pub fn way_point_at(account_id: &str, checkpoints: &[u32], finished: bool) -> Event {
    let racetime = checkpoints.last().copied().unwrap_or(0);
    let checkpoint = checkpoints.len().saturating_sub(1) as u32;
    Event::WayPoint(WayPoint {
        account_id: account_id.into(),
        time: 0,
        racetime,
        laptime: racetime,
        speed: 0.,
        checkpoint_in_race: checkpoint,
        checkpoint_in_lap: checkpoint,
        is_end_race: finished,
        is_end_lap: finished,
        is_infinite_laps: false,
        is_independent_laps: false,
        current_race_checkpoints: checkpoints.to_vec(),
        current_lap_checkpoints: checkpoints.to_vec(),
        block_id: String::new(),
    })
}

/// A player finished the race in this time at a checkpoint, counting from 0.
pub fn finish(account_id: &str, checkpoint_in_race: u32, racetime: u32) -> Event {
    let mut checkpoints: Vec<u32> = (1..=checkpoint_in_race + 1)
        .map(|checkpoint| checkpoint * 10_000)
        .collect();
    if let Some(finish) = checkpoints.last_mut() {
        *finish = racetime;
    }
    way_point_at(account_id, &checkpoints, true)
}

/// A player starts a race and finishes it in this time.
pub fn race(account_id: &str, racetime: u32) -> Vec<Event> {
    vec![start_line(account_id), finish(account_id, 0, racetime)]
}

/// A player on the server.
pub fn player(account_id: &str, name: &str) -> PlayerInfo {
    PlayerInfo {
        account_id: account_id.into(),
        nick_name: name.into(),
        player_id: 0,
        team_id: -1,
        spectator_status: 0,
        ladder_ranking: 0,
        flags: 0,
    }
}

/// The server, which shows up as a player too.
pub fn server() -> PlayerInfo {
    PlayerInfo {
        flags: 0b10_0000,
        ..player("server", "Server")
    }
}

/// The server tells about a player on it, e.g. once they connected.
pub fn player_info(account_id: &str, name: &str) -> Event {
    Event::PlayerInfoChanged(player(account_id, name))
}

/// Handles every event and returns the reply to the last one.
pub fn play<'a>(driver: &mut Driver, events: impl IntoIterator<Item = &'a Event>) -> DriverReply {
    let mut reply = DriverReply::default();
    for event in events {
        reply = driver.handle(event).unwrap();
    }
    reply
}

/// The events as inputs of a driver, e.g. to replay them.
pub fn inputs<'a>(events: impl IntoIterator<Item = &'a Event>) -> Vec<DriverInput> {
    events
        .into_iter()
        .cloned()
        .map(DriverInput::Event)
        .collect()
}

/// What modes send at map start without a warmup in their settings.
pub fn no_warm_up() -> DriverCommand {
    DriverCommand::SetWarmUp(DriverWarmUp::default())
}

/// The commands of a reply without what the host publishes for the manialinks.
pub fn without_published(commands: &[DriverCommand]) -> Vec<DriverCommand> {
    commands
        .iter()
        .filter(|c| {
            !matches!(
                c,
                DriverCommand::SetStandings(_)
                    | DriverCommand::SetRound(_)
                    | DriverCommand::SetUsers(_)
            )
        })
        .cloned()
        .collect()
}

pub fn publishes_standings(commands: &[DriverCommand]) -> bool {
    commands
        .iter()
        .any(|c| matches!(c, DriverCommand::SetStandings(_)))
}

/// Account, position, status, label, points and points of the last round of every
/// player in the order of the standings.
pub fn ranking(driver: &Driver) -> Vec<(String, u32, DriverStatus, String, i32, i32)> {
    let round = driver.last_round();
    driver
        .standings()
        .entries
        .into_iter()
        .map(|p| {
            let round_score = round
                .entries
                .iter()
                .find(|r| r.account_id == p.account_id)
                .map_or(0, |r| r.score);
            (
                p.account_id,
                p.position,
                p.status,
                p.label,
                p.score,
                round_score,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_number_their_holds_and_order_their_races() {
        let mut log = InputLog::new("{}");
        log.match_start().map_start("m1");
        log.round(&[
            ("a", Race::finish(&[10_000, 30_000])),
            ("b", Race::dnf(&[9_000])),
        ]);
        log.hold_again().restart_script().map_start("m1");

        let summary: Vec<String> = log
            .inputs
            .iter()
            .map(|input| match input {
                DriverInput::Event(Event::DriverHold(hold)) => {
                    format!("hold {} {}", hold.session, hold.seq)
                }
                DriverInput::Event(Event::StartLine(start)) => {
                    format!("start {}", start.account_id)
                }
                DriverInput::Event(Event::WayPoint(way_point)) => {
                    format!("{} {}", way_point.account_id, way_point.racetime)
                }
                DriverInput::Event(event) => format!("{event:?}").chars().take(12).collect(),
                _ => String::new(),
            })
            .collect();
        assert_eq!(
            summary,
            [
                "StartMatchSt",
                "StartMapStar",
                "hold 7 1",
                "StartRoundSt",
                "start a",
                "start b",
                "b 9000",
                "a 10000",
                "a 30000",
                "hold 7 2",
                "hold 7 2",
                "StartMatchSt",
                "StartMapStar",
                "hold 8 1",
            ]
        );
    }

    #[test]
    fn logs_are_json() {
        let mut log = InputLog::new("{}");
        log.players(&["a"]).match_start();
        let json = serde_json::to_string(&log).unwrap();
        let back: InputLog = serde_json::from_str(&json).unwrap();
        assert_eq!(back.settings, "{}");
        assert_eq!(back.inputs.len(), 2);
    }
}
