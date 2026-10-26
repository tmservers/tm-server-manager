//! Builds the first-party modes and the callbacks of the server for the integration tests.
#![allow(dead_code)]

use std::{path::PathBuf, process::Command};

use tm_server_types::{
    base::{Map, Player, RoundTime},
    event::{
        DriverHold, DriverSection, Event, Scores, StartLine, StartMap, StartMatch, StartRound,
        WayPoint,
    },
    method::{DriverCommand, DriverStatus},
};
use tm_wasm_driver::Driver;

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

/// The scores the modescript sends right before the round end hold.
/// Every player has the time of the last and of the best race in milliseconds, -1 for none.
pub fn scores(players: &[(&str, i32, i32)]) -> Event {
    scores_of_section("PreEndRound", players)
}

/// The scores the modescript sends when a round starts, so the host knows the names.
pub fn round_start_scores(players: &[&str]) -> Event {
    let players: Vec<(&str, i32, i32)> = players.iter().map(|&p| (p, -1, -1)).collect();
    scores_of_section("", &players)
}

fn scores_of_section(section: &str, players: &[(&str, i32, i32)]) -> Event {
    Event::Scores(Scores {
        response_id: String::new(),
        section: section.into(),
        use_teams: false,
        winner_team: -1,
        winner_player: String::new(),
        teams: vec![],
        players: players
            .iter()
            .map(|(account_id, previous, best)| Player {
                best_race_checkpoints: checkpoints(*best),
                previous_race_checkpoints: checkpoints(*previous),
                best_lap_checkpoints: vec![],
                account_id: account_id.to_string(),
                name: account_id.to_string(),
                team: -1,
                rank: 0,
                round_points: 0,
                map_points: 0,
                match_points: 0,
                best_racetime: race_time(*best),
                best_laptime: RoundTime::None,
                previous_racetime: race_time(*previous),
            })
            .collect(),
    })
}

fn race_time(time: i32) -> RoundTime {
    if time < 0 {
        RoundTime::None
    } else {
        RoundTime::Time(time as u32)
    }
}

fn checkpoints(time: i32) -> Vec<u32> {
    if time < 0 { vec![] } else { vec![time as u32] }
}

/// The commands of a reply without the standings the host publishes.
pub fn without_standings(commands: &[DriverCommand]) -> Vec<DriverCommand> {
    commands
        .iter()
        .filter(|c| !matches!(c, DriverCommand::SetStandings(_)))
        .cloned()
        .collect()
}

pub fn publishes_standings(commands: &[DriverCommand]) -> bool {
    commands
        .iter()
        .any(|c| matches!(c, DriverCommand::SetStandings(_)))
}

/// Account, rank, status, points and round points of every player in the order of the standings.
pub fn ranking(driver: &Driver) -> Vec<(String, u32, DriverStatus, i32, i32)> {
    driver
        .standings()
        .players
        .into_iter()
        .map(|p| (p.account_id, p.rank, p.status, p.points, p.round_points))
        .collect()
}
