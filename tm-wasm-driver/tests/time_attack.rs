//! Runs the first-party time attack mode through the host as a real wasm module.

mod common;

use common::{
    TIME_ATTACK, hold, match_start, play, player_info, publishes_standings, race, round_start,
    without_published,
};
use tm_server_types::{
    event::{DriverPlayerResult, DriverRoundResult, DriverSection},
    method::{DriverCommand, DriverRoundFormat},
};
use tm_wasm_driver::{Driver, DriverError};

fn time_attack(settings: &str) -> Result<Driver, DriverError> {
    let (engine, module) = common::compile(TIME_ATTACK);
    engine.instantiate(&module, settings)
}

#[test]
fn plays_a_match() {
    let mut driver = time_attack(r#"{"time_limit_ms": 60000}"#).unwrap();

    driver.handle(&match_start()).unwrap();
    assert_eq!(
        without_published(
            &driver
                .handle(&hold(1, DriverSection::MapStart))
                .unwrap()
                .commands
        ),
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::TimeAttack(60_000)),
            common::no_warm_up(),
            DriverCommand::Continue(1)
        ]
    );

    // The time limit ran out. Time attack ranks by the best race, not the last one.
    // c is on the server without racing.
    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 42_000));
    play(&mut driver, &race("b", 41_000));
    play(&mut driver, &race("b", 45_000));
    driver.handle(&player_info("c", "c")).unwrap();
    let reply = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert!(publishes_standings(&reply.commands));
    assert_eq!(
        without_published(&reply.commands),
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
        without_published(
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
