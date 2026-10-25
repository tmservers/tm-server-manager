//! Runs the first-party time attack mode through the host as a real wasm module.

mod common;

use std::sync::OnceLock;

use tm_server_types::{
    event::{
        DriverEvent, DriverMapEnd, DriverMapStart, DriverMatchStart, DriverRaceResult,
        DriverRoundEnd, DriverRoundStart,
    },
    method::{DriverCommand, DriverPlayerPoints, DriverRoundFormat},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverError, DriverLimits};

const SESSION: i32 = 3;

/// Builds the time attack mode once per test binary.
fn time_attack_wasm() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| common::build_mode("tm-driver-mode-time-attack"))
}

fn time_attack(settings: &str) -> Result<Driver, DriverError> {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(time_attack_wasm()).unwrap();
    engine.instantiate(&module, settings)
}

fn points(account_id: &str, round: i32, total: i32) -> DriverCommand {
    DriverCommand::SetPlayerPoints(DriverPlayerPoints {
        account_id: account_id.into(),
        round_points: Some(round),
        map_points: Some(round),
        match_points: Some(total),
    })
}

#[test]
fn plays_a_match() {
    let mut driver = time_attack(r#"{"time_limit_ms": 60000}"#).unwrap();
    let format = DriverCommand::SetRoundFormat(DriverRoundFormat::TimeAttack(60_000));

    let match_start = DriverEvent::MatchStart(DriverMatchStart {
        session: SESSION,
        seq: 1,
        time: 0,
        players: vec![],
    });
    assert_eq!(
        driver.handle(&match_start).unwrap(),
        [format.clone(), DriverCommand::Ack(1)]
    );

    let map_start = DriverEvent::MapStart(DriverMapStart {
        session: SESSION,
        seq: 2,
        time: 0,
        map_uid: "olsKnq_qAghcVAnEkoeUnVHFZei".into(),
        map_name: "Training - 01".into(),
        players: vec![],
    });
    assert_eq!(
        driver.handle(&map_start).unwrap(),
        [format, DriverCommand::Ack(2)]
    );

    let round_start = DriverEvent::RoundStart(DriverRoundStart {
        session: SESSION,
        seq: 3,
        time: 0,
        round: 1,
        players: vec![],
    });
    assert_eq!(
        driver.handle(&round_start).unwrap(),
        [DriverCommand::Ack(3)]
    );

    // The modescript reports the best time of every player once the time limit ran out.
    let round_end = DriverEvent::RoundEnd(DriverRoundEnd {
        session: SESSION,
        seq: 4,
        time: 60_000,
        round: 1,
        forced: false,
        results: [("b", 41_000), ("a", 42_000), ("c", -1)]
            .into_iter()
            .map(|(account_id, time)| DriverRaceResult {
                account_id: account_id.into(),
                time,
                checkpoints: vec![],
            })
            .collect(),
    });
    assert_eq!(
        driver.handle(&round_end).unwrap(),
        [
            points("b", 3, 3),
            points("a", 2, 2),
            DriverCommand::EndMap,
            DriverCommand::Ack(4)
        ]
    );

    let map_end = DriverEvent::MapEnd(DriverMapEnd {
        session: SESSION,
        seq: 5,
        time: 61_000,
    });
    assert_eq!(
        driver.handle(&map_end).unwrap(),
        [DriverCommand::EndMatch, DriverCommand::Ack(5)]
    );
}

#[test]
fn rejects_a_zero_time_limit() {
    assert!(matches!(
        time_attack(r#"{"time_limit_ms": 0}"#),
        Err(DriverError::Rejected(_))
    ));
}
