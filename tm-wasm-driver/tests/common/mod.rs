//! Builds the first-party modes and the callbacks of the server for the integration tests.
#![allow(dead_code)]

use std::{path::PathBuf, process::Command};

use tm_server_types::{
    base::{Player, RoundTime},
    event::{DriverHold, DriverSection, Event, Scores, StartMatch, StartRound},
};

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

pub fn round_start(count: u32) -> Event {
    Event::StartRoundStart(StartRound { count, time: 0 })
}

/// The scores the modescript sends right before the round end hold.
/// Every player has the time of the last and of the best race in milliseconds, -1 for none.
pub fn scores(players: &[(&str, i32, i32)]) -> Event {
    Event::Scores(Scores {
        response_id: String::new(),
        section: "PreEndRound".into(),
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
