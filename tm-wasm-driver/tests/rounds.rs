//! Runs the first-party rounds mode through the host as a real wasm module.

mod common;

use std::sync::OnceLock;

use tm_server_types::{
    event::{
        DriverEvent, DriverHello, DriverMapEnd, DriverMapStart, DriverMatchStart, DriverPlayer,
        DriverPlayerJoin, DriverRaceResult, DriverRoundEnd, DriverRoundStart,
    },
    method::{DriverCommand, DriverFinishTimeout, DriverPlayerPoints, DriverRoundFormat},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverError, DriverLimits};

const SESSION: i32 = 7;

/// Builds the rounds mode once per test binary.
fn rounds_wasm() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| common::build_mode("tm-driver-mode-rounds"))
}

fn rounds(settings: &str) -> Driver {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(rounds_wasm()).unwrap();
    engine.instantiate(&module, settings).unwrap()
}

fn match_start(seq: u32) -> DriverEvent {
    DriverEvent::MatchStart(DriverMatchStart {
        session: SESSION,
        seq,
        time: 0,
        players: vec![],
    })
}

fn map_start(seq: u32) -> DriverEvent {
    DriverEvent::MapStart(DriverMapStart {
        session: SESSION,
        seq,
        time: 0,
        map_uid: "olsKnq_qAghcVAnEkoeUnVHFZei".into(),
        map_name: "Training - 01".into(),
        players: vec![],
    })
}

fn round_start(seq: u32, round: u32) -> DriverEvent {
    DriverEvent::RoundStart(DriverRoundStart {
        session: SESSION,
        seq,
        time: 0,
        round,
        players: vec![],
    })
}

fn round_end(seq: u32, round: u32, results: &[(&str, i32)]) -> DriverEvent {
    DriverEvent::RoundEnd(DriverRoundEnd {
        session: SESSION,
        seq,
        time: 0,
        round,
        forced: false,
        results: results
            .iter()
            .map(|(account_id, time)| DriverRaceResult {
                account_id: account_id.to_string(),
                time: *time,
                checkpoints: vec![],
            })
            .collect(),
    })
}

fn points(account_id: &str, round: i32, map: i32, total: i32) -> DriverCommand {
    DriverCommand::SetPlayerPoints(DriverPlayerPoints {
        account_id: account_id.into(),
        round_points: Some(round),
        map_points: Some(map),
        match_points: Some(total),
    })
}

#[test]
fn plays_a_match() {
    let mut driver = rounds(r#"{"points_repartition": [10, 6], "rounds_per_map": 1}"#);

    let hello = DriverEvent::Hello(DriverHello {
        session: SESSION,
        seq: 1,
        time: 0,
        script_version: "0.1.0".into(),
    });
    assert_eq!(driver.handle(&hello).unwrap(), []);

    assert_eq!(
        driver.handle(&match_start(2)).unwrap(),
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt),
            DriverCommand::SetFinishTimeout(DriverFinishTimeout::Automatic),
            DriverCommand::Ack(2)
        ]
    );
    assert_eq!(
        driver.handle(&map_start(3)).unwrap(),
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt),
            DriverCommand::SetFinishTimeout(DriverFinishTimeout::Automatic),
            DriverCommand::Ack(3)
        ]
    );
    assert_eq!(
        driver.handle(&round_start(4, 1)).unwrap(),
        [DriverCommand::Ack(4)]
    );
    assert_eq!(
        driver
            .handle(&round_end(5, 1, &[("a", 30_000), ("b", 31_000), ("c", -1)]))
            .unwrap(),
        [
            points("a", 10, 10, 10),
            points("b", 6, 6, 6),
            DriverCommand::EndMap,
            DriverCommand::Ack(5)
        ]
    );

    // Player join is no decision so it does not get acknowledged.
    let join = DriverEvent::PlayerJoin(DriverPlayerJoin {
        session: SESSION,
        seq: 6,
        time: 0,
        player: DriverPlayer {
            account_id: "a".into(),
            name: "a".into(),
            team: -1,
            spectator: false,
        },
    });
    assert_eq!(
        driver.handle(&join).unwrap(),
        [DriverCommand::SetPlayerPoints(DriverPlayerPoints {
            account_id: "a".into(),
            round_points: None,
            map_points: Some(10),
            match_points: Some(10),
        })]
    );

    let map_end = DriverEvent::MapEnd(DriverMapEnd {
        session: SESSION,
        seq: 7,
        time: 0,
    });
    assert_eq!(
        driver.handle(&map_end).unwrap(),
        [DriverCommand::EndMatch, DriverCommand::Ack(7)]
    );
}

#[test]
fn repeated_decisions_are_applied_once() {
    let mut driver = rounds(r#"{"rounds_per_map": 0}"#);
    driver.handle(&match_start(1)).unwrap();
    driver.handle(&map_start(2)).unwrap();

    let first = driver.handle(&round_end(3, 1, &[("a", 30_000)])).unwrap();
    let repeated = driver.handle(&round_end(3, 1, &[("a", 30_000)])).unwrap();
    assert_eq!(first, repeated);

    // The repeated callback did not add points again.
    assert_eq!(
        driver.handle(&round_end(4, 2, &[("a", 30_000)])).unwrap(),
        [points("a", 10, 20, 20), DriverCommand::Ack(4)]
    );

    // An older decision than the last one is stale.
    assert_eq!(
        driver.handle(&round_end(3, 1, &[("a", 30_000)])).unwrap(),
        []
    );
}

#[test]
fn new_session_starts_over() {
    let mut driver = rounds("");
    driver.handle(&match_start(1)).unwrap();
    driver.handle(&map_start(2)).unwrap();

    // The modescript restarted and counts from 1 again.
    let restarted = DriverEvent::MatchStart(DriverMatchStart {
        session: SESSION + 1,
        seq: 1,
        time: 0,
        players: vec![],
    });
    assert_eq!(
        driver.handle(&restarted).unwrap(),
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt),
            DriverCommand::SetFinishTimeout(DriverFinishTimeout::Automatic),
            DriverCommand::Ack(1)
        ]
    );
}

#[test]
fn resumes_from_a_snapshot() {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(rounds_wasm()).unwrap();
    let mut driver = engine
        .instantiate(&module, r#"{"rounds_per_map": 0}"#)
        .unwrap();
    driver.handle(&match_start(1)).unwrap();
    driver.handle(&map_start(2)).unwrap();
    let decided = driver.handle(&round_end(3, 1, &[("a", 30_000)])).unwrap();

    // Snapshots survive a serialization round trip like they would in a database.
    let snapshot = serde_json::to_vec(&driver.snapshot().unwrap()).unwrap();
    drop(driver);
    let mut resumed = engine
        .restore(&module, serde_json::from_slice(&snapshot).unwrap())
        .unwrap();

    // The modescript asks again because the bridge went away before it answered.
    assert_eq!(
        resumed.handle(&round_end(3, 1, &[("a", 30_000)])).unwrap(),
        decided
    );
    assert_eq!(
        resumed.handle(&round_end(4, 2, &[("a", 30_000)])).unwrap(),
        [points("a", 10, 20, 20), DriverCommand::Ack(4)]
    );
}

#[test]
fn rejects_invalid_settings() {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(rounds_wasm()).unwrap();
    assert!(matches!(
        engine.instantiate(&module, r#"{"rounds_per_mpa": 3}"#),
        Err(DriverError::Rejected(_))
    ));
}

#[test]
fn runs_out_of_fuel() {
    let engine = DriverEngine::new(DriverLimits {
        fuel_per_call: 1_000,
        ..Default::default()
    })
    .unwrap();
    let module = engine.compile(rounds_wasm()).unwrap();
    let error = match engine.instantiate(&module, "") {
        Err(error @ (DriverError::Trap(_) | DriverError::Instantiate(_))) => error,
        Err(error) => panic!("Unexpected error: {error}"),
        Ok(_) => panic!("The mode should have run out of fuel"),
    };
    assert!(error.to_string().contains("fuel"), "{error}");
}

#[test]
fn rejects_non_wasm() {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    assert!(matches!(
        engine.compile(b"definitely not wasm"),
        Err(DriverError::InvalidModule(_))
    ));
}
