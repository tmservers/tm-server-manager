//! Runs the first-party rounds mode through the host as a real wasm module.

mod common;

use std::sync::OnceLock;

use common::{SESSION, forced_round_end, hold, match_start, round_start, scores};
use tm_server_types::{
    event::{DriverHold, DriverPlayerResult, DriverRoundResult, DriverSection, Event},
    method::{DriverCommand, DriverFinishTimeout, DriverRoundFormat},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverError, DriverLimits, DriverReply};

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

fn result(account_id: &str, points: i32, position: u32) -> DriverPlayerResult {
    DriverPlayerResult {
        account_id: account_id.into(),
        points,
        position,
    }
}

#[test]
fn plays_a_match() {
    let mut driver = rounds(r#"{"points_repartition": [10, 6], "rounds_per_map": 1}"#);

    // Informative callbacks need no reply.
    assert_eq!(
        driver.handle(&match_start()).unwrap(),
        DriverReply::default()
    );
    assert_eq!(
        driver
            .handle(&hold(1, DriverSection::MapStart))
            .unwrap()
            .commands,
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt(
                DriverFinishTimeout::Automatic
            )),
            DriverCommand::Continue(1)
        ]
    );
    assert_eq!(
        driver.handle(&round_start(1)).unwrap(),
        DriverReply::default()
    );

    // b has the better best time but rounds only count the last race.
    driver
        .handle(&scores(&[
            ("a", 30_000, 30_000),
            ("b", 31_000, 29_000),
            ("c", -1, -1),
        ]))
        .unwrap();
    let reply = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(
        reply.commands,
        [DriverCommand::EndMap, DriverCommand::Continue(2)]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", 10, 1), result("b", 6, 2)]
        })
    );

    assert_eq!(
        driver
            .handle(&hold(3, DriverSection::MapEnd))
            .unwrap()
            .commands,
        [DriverCommand::EndMatch, DriverCommand::Continue(3)]
    );
}

#[test]
fn repeated_holds_are_decided_once() {
    // The match ends once a player has more than 25 points.
    let mut driver = rounds(r#"{"rounds_per_map": 0, "points_limit": 25, "use_tie_break": false}"#);
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();

    driver.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();
    let first = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    // The modescript sends the scores again with every repeated hold.
    driver.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();
    let repeated = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(repeated.commands, first.commands);
    // The result was already reported with the first reply.
    assert_eq!(repeated.round_result, None);

    // a has 20 points now. With the repeated hold counted it would be 30.
    driver.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();
    assert_eq!(
        driver
            .handle(&hold(3, DriverSection::RoundEnd))
            .unwrap()
            .commands,
        [DriverCommand::Continue(3)]
    );

    // An older hold than the last one is stale.
    assert_eq!(
        driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap(),
        DriverReply::default()
    );
}

#[test]
fn forced_rounds_have_no_result() {
    let mut driver = rounds("");
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();

    let reply = driver.handle(&forced_round_end(2)).unwrap();
    assert_eq!(reply.commands, [DriverCommand::Continue(2)]);
    assert_eq!(reply.round_result, None);
}

#[test]
fn new_session_starts_over() {
    let mut driver = rounds("");
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&hold(2, DriverSection::MapEnd)).unwrap();

    // The modescript restarted and counts from 1 again.
    let restarted = Event::DriverHold(DriverHold {
        session: SESSION + 1,
        seq: 1,
        section: DriverSection::MapStart,
        forced: false,
    });
    assert_eq!(
        driver.handle(&restarted).unwrap().commands.last(),
        Some(&DriverCommand::Continue(1))
    );
}

#[test]
fn resumes_from_a_snapshot() {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(rounds_wasm()).unwrap();
    let mut driver = engine
        .instantiate(
            &module,
            r#"{"rounds_per_map": 0, "points_limit": 25, "use_tie_break": false}"#,
        )
        .unwrap();
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();
    let decided = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();

    // Snapshots survive a serialization round trip like they would in a database.
    let snapshot = serde_json::to_vec(&driver.snapshot().unwrap()).unwrap();
    drop(driver);
    let mut resumed = engine
        .restore(&module, serde_json::from_slice(&snapshot).unwrap())
        .unwrap();

    // The modescript asks again because the bridge went away before it answered.
    driver_scores_and_hold(&mut resumed, 2, &decided.commands);

    // The points of the first round survived: 10 + 10 + 10 reaches the limit in the third round.
    resumed.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();
    resumed.handle(&hold(3, DriverSection::RoundEnd)).unwrap();
    resumed.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();
    assert_eq!(
        resumed
            .handle(&hold(4, DriverSection::RoundEnd))
            .unwrap()
            .commands,
        [DriverCommand::EndMatch, DriverCommand::Continue(4)]
    );
}

fn driver_scores_and_hold(driver: &mut Driver, seq: u32, expected: &[DriverCommand]) {
    driver.handle(&scores(&[("a", 30_000, 30_000)])).unwrap();
    assert_eq!(
        driver
            .handle(&hold(seq, DriverSection::RoundEnd))
            .unwrap()
            .commands,
        expected
    );
}

#[test]
fn ignores_unrelated_callbacks() {
    let mut driver = rounds("");
    assert!(!Driver::handles(&Event::WarmupStart));
    assert_eq!(
        driver.handle(&Event::WarmupStart).unwrap(),
        DriverReply::default()
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
