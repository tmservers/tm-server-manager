//! Builds the first-party modes and the callbacks of the server for the integration tests.
#![allow(dead_code)]

use std::{collections::BTreeMap, path::PathBuf, process::Command, sync::Mutex};

use tm_server_types::{
    base::{Map, PlayerInfo},
    event::{
        DriverHold, DriverSection, Event, StartLine, StartMap, StartMatch, StartRound, WayPoint,
    },
    method::{DriverCommand, DriverStatus, DriverWarmUp},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverLimits, DriverReply};
use wasmtime::component::Component;

pub const SESSION: i32 = 7;

pub const ROUNDS: &str = "tm-driver-mode-rounds";
pub const TIME_ATTACK: &str = "tm-driver-mode-time-attack";
pub const REVERSE_CUP: &str = "tm-driver-mode-reverse-cup";
pub const KNOCKOUT: &str = "tm-driver-mode-knockout";

/// A mode crate of the workspace as a wasm module, built once per test binary.
pub fn mode(package: &'static str) -> &'static [u8] {
    static BUILT: Mutex<BTreeMap<&str, &[u8]>> = Mutex::new(BTreeMap::new());
    // Held while building, so tests running in parallel wait for the first build.
    let mut built = BUILT.lock().unwrap();
    built
        .entry(package)
        .or_insert_with(|| build_mode(package).leak())
}

/// Compiles a mode crate of the workspace with the default limits.
pub fn compile(package: &'static str) -> (DriverEngine, Component) {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(mode(package)).unwrap();
    (engine, module)
}

/// Starts a mode crate of the workspace with these settings.
pub fn driver(package: &'static str, settings: &str) -> Driver {
    let (engine, module) = compile(package);
    engine.instantiate(&module, settings).unwrap()
}

/// Builds a mode crate of the workspace for wasm32-unknown-unknown and returns the module.
fn build_mode(package: &str) -> Vec<u8> {
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

/// A player passed a checkpoint of the race, counting from 0, ten seconds after the
/// one before. Like the server, it carries the race times of every checkpoint so far.
pub fn way_point(account_id: &str, checkpoint_in_race: u32) -> Event {
    let checkpoints: Vec<u32> = (1..=checkpoint_in_race + 1)
        .map(|checkpoint| checkpoint * 10_000)
        .collect();
    Event::WayPoint(WayPoint {
        account_id: account_id.into(),
        time: 0,
        racetime: checkpoints[checkpoints.len() - 1],
        laptime: 0,
        speed: 0.,
        checkpoint_in_race,
        checkpoint_in_lap: checkpoint_in_race,
        is_end_race: false,
        is_end_lap: false,
        is_infinite_laps: false,
        is_independent_laps: false,
        current_race_checkpoints: checkpoints,
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
    if let Some(finish) = way_point.current_race_checkpoints.last_mut() {
        *finish = racetime;
    }
    Event::WayPoint(way_point)
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
        flags: 0b100000,
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
