//! Runs the first-party time attack mode through the host as a real wasm module.

mod common;

use std::sync::OnceLock;

use common::{hold, match_start, publishes_standings, scores, without_standings};
use tm_server_types::{
    event::{DriverPlayerResult, DriverRoundResult, DriverSection},
    method::{DriverCommand, DriverRoundFormat},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverError, DriverLimits};

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

#[test]
fn plays_a_match() {
    let mut driver = time_attack(r#"{"time_limit_ms": 60000}"#).unwrap();

    driver.handle(&match_start()).unwrap();
    assert_eq!(
        without_standings(
            &driver
                .handle(&hold(1, DriverSection::MapStart))
                .unwrap()
                .commands
        ),
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::TimeAttack(60_000)),
            DriverCommand::Continue(1)
        ]
    );

    // The time limit ran out. Time attack ranks by the best race, not the last one.
    driver
        .handle(&scores(&[
            ("a", 42_000, 42_000),
            ("b", 45_000, 41_000),
            ("c", -1, -1),
        ]))
        .unwrap();
    let reply = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert!(publishes_standings(&reply.commands));
    assert_eq!(
        without_standings(&reply.commands),
        [DriverCommand::EndMap, DriverCommand::Continue(2)]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![
                DriverPlayerResult {
                    account_id: "b".into(),
                    points: 2,
                    position: 1,
                },
                DriverPlayerResult {
                    account_id: "a".into(),
                    points: 1,
                    position: 2,
                },
            ]
        })
    );

    assert_eq!(
        without_standings(
            &driver
                .handle(&hold(3, DriverSection::MapEnd))
                .unwrap()
                .commands
        ),
        [DriverCommand::EndMatch, DriverCommand::Continue(3)]
    );
}

#[test]
fn rejects_a_zero_time_limit() {
    assert!(matches!(
        time_attack(r#"{"time_limit_ms": 0}"#),
        Err(DriverError::Rejected(_))
    ));
}
