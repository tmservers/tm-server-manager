//! Builds the first-party modes and the callbacks of the server for the integration tests.
#![allow(dead_code)]

use std::{path::PathBuf, process::Command};

use tm_server_types::{
    base::{Map, PlayerInfo},
    event::{
        DriverHold, DriverSection, Event, StartLine, StartMap, StartMatch, StartRound, WayPoint,
    },
    method::{DriverCommand, DriverStatus, DriverWarmUp},
};
use tm_wasm_driver::{Driver, DriverReply};

pub const SESSION: i32 = 7;

/// Builds a mode crate of the workspace for wasm32-unknown-unknown and returns the module.
pub fn build_mode(package: &str) -> Vec<u8> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let target_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("modes");
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .current_dir(&workspace)
        .args([
            "build",
            "-p",
            package,
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--target-dir",
        ])
        .arg(&target_dir)
        .status()
        .expect("cargo could not be started");
    assert!(status.success(), "Building {package} failed");

    let file = format!("{}.wasm", package.replace('-', "_"));
    std::fs::read(
        target_dir
            .join("wasm32-unknown-unknown/release")
            .join(&file),
    )
    .unwrap_or_else(|_| panic!("{file} was not built"))
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

/// A player passed a checkpoint of the race, counting from 0.
pub fn way_point(account_id: &str, checkpoint_in_race: u32) -> Event {
    Event::WayPoint(WayPoint {
        account_id: account_id.into(),
        time: 0,
        racetime: 0,
        laptime: 0,
        speed: 0.,
        checkpoint_in_race,
        checkpoint_in_lap: checkpoint_in_race,
        is_end_race: false,
        is_end_lap: false,
        is_infinite_laps: false,
        is_independent_laps: false,
        current_race_checkpoints: vec![],
        current_lap_checkpoints: vec![],
        block_id: String::new(),
    })
}

/// A player finished the race in this time at a checkpoint, counting from 0.
pub fn finish(account_id: &str, checkpoint_in_race: u32, racetime: u32) -> Event {
    let Event::WayPoint(mut way_point) = way_point(account_id, checkpoint_in_race) else {
        unreachable!()
    };
    way_point.racetime = racetime;
    way_point.is_end_race = true;
    way_point.is_end_lap = true;
    way_point.current_race_checkpoints = vec![racetime];
    Event::WayPoint(way_point)
}

/// A player starts a race and finishes it in this time.
pub fn race(account_id: &str, racetime: u32) -> Vec<Event> {
    vec![start_line(account_id), finish(account_id, 0, racetime)]
}

/// The server tells about a player on it, e.g. once they connected.
pub fn player_info(account_id: &str, name: &str) -> Event {
    Event::PlayerInfoChanged(PlayerInfo {
        account_id: account_id.into(),
        nick_name: name.into(),
        player_id: 0,
        team_id: -1,
        spectator_status: 0,
        ladder_ranking: 0,
        flags: 0,
    })
}

/// Handles every event and returns the reply to the last one.
pub fn play<'a>(driver: &mut Driver, events: impl IntoIterator<Item = &'a Event>) -> DriverReply {
    let mut reply = DriverReply::default();
    for event in events {
        reply = driver.handle(event).unwrap();
    }
    reply
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
