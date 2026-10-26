//! Runs the first-party reverse cup mode through the host as a real wasm module.

mod common;

use std::sync::OnceLock;

use common::{
    hold, match_start, ranking, round_start, scores, start_line, way_point, without_standings,
};
use tm_server_types::{
    event::{DriverPlayerResult, DriverRoundResult, DriverSection, Event},
    method::{DriverCanSpawn, DriverCommand, DriverFinishTimeout, DriverRoundFormat, DriverStatus},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverLimits, DriverReply};

/// Builds the reverse cup mode once per test binary.
fn reverse_cup_wasm() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| common::build_mode("tm-driver-mode-reverse-cup"))
}

const SETTINGS: &str =
    r#"{"starting_points": 10, "last_chance_dnf_mode": "only_least_checkpoints"}"#;

fn result(account_id: &str, points: i32, position: u32) -> DriverPlayerResult {
    DriverPlayerResult {
        account_id: account_id.into(),
        points,
        position,
    }
}

fn cannot_spawn(account_id: &str) -> DriverCommand {
    DriverCommand::SetCanSpawn(DriverCanSpawn {
        account_id: account_id.into(),
        can_spawn: false,
    })
}

/// A round in which a finishes, b gives up after two checkpoints and c after one.
/// s only watches.
fn round_with_give_ups(round: u32, seq: u32) -> Vec<Event> {
    vec![
        round_start(round),
        start_line("a"),
        start_line("b"),
        start_line("c"),
        way_point("a", 0),
        way_point("b", 0),
        way_point("c", 0),
        way_point("a", 1),
        way_point("b", 1),
        way_point("a", 2),
        scores(&[
            ("a", 30_000, 30_000),
            ("b", -1, -1),
            ("c", -1, -1),
            ("s", -1, -1),
        ]),
        hold(seq, DriverSection::RoundEnd),
    ]
}

/// A round in which a finishes before b.
fn final_round(round: u32, seq: u32) -> Vec<Event> {
    vec![
        round_start(round),
        start_line("a"),
        start_line("b"),
        way_point("a", 0),
        way_point("b", 0),
        scores(&[
            ("a", 30_000, 30_000),
            ("b", 32_000, 32_000),
            ("c", -1, -1),
            ("s", -1, -1),
        ]),
        hold(seq, DriverSection::RoundEnd),
    ]
}

/// Handles every event and returns the reply to the last one.
fn play(driver: &mut Driver, events: &[Event]) -> DriverReply {
    let mut reply = DriverReply::default();
    for event in events {
        reply = driver.handle(event).unwrap();
    }
    reply
}

#[test]
fn plays_a_match() {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(reverse_cup_wasm()).unwrap();
    let mut driver = engine.instantiate(&module, SETTINGS).unwrap();

    driver.handle(&match_start()).unwrap();
    assert_eq!(
        without_standings(
            &driver
                .handle(&hold(1, DriverSection::MapStart))
                .unwrap()
                .commands
        ),
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt(
                DriverFinishTimeout::Automatic
            )),
            DriverCommand::Continue(1)
        ]
    );

    // The players who start the first round take part. Three players lose 2, 6
    // and 9 points, giving up costs 20 and puts b and c in last chance.
    let reply = play(&mut driver, &round_with_give_ups(1, 2));
    assert_eq!(
        without_standings(&reply.commands),
        [cannot_spawn("s"), DriverCommand::Continue(2)]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", -2, 1), result("b", -20, 2), result("c", -20, 3)]
        })
    );

    // In last chance only c, who passed the fewest checkpoints, loses points.
    let reply = play(&mut driver, &round_with_give_ups(2, 3));
    assert_eq!(
        without_standings(&reply.commands),
        [
            DriverCommand::ChatMessage("$f90$i$<c$> takes 3rd place!".into()),
            cannot_spawn("c"),
            DriverCommand::Continue(3)
        ]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", -2, 1), result("b", 0, 2), result("c", -20, 3)]
        })
    );

    // Two players lose 4 and 8 points, b loses the most and a wins.
    let reply = play(&mut driver, &final_round(3, 4));
    assert_eq!(
        without_standings(&reply.commands),
        [
            DriverCommand::ChatMessage("$f90$i$<b$> takes 2nd place!".into()),
            cannot_spawn("b"),
            DriverCommand::ChatMessage("$<a$> wins the match!".into()),
            DriverCommand::EndMatch,
            DriverCommand::Continue(4)
        ]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", -4, 1), result("b", -8, 2)]
        })
    );
    assert_eq!(
        ranking(&driver),
        [
            ("a".into(), 1, DriverStatus::Winner, 2, -4),
            ("b".into(), 2, DriverStatus::Eliminated, 0, -8),
            ("c".into(), 3, DriverStatus::Eliminated, 0, 0),
            // Watched without taking part.
            ("s".into(), 0, DriverStatus::Spectator, 0, 0),
        ]
    );
    assert_eq!(driver.standings().info, "1 left, 3 / 5 rounds");

    assert_eq!(
        without_standings(
            &driver
                .handle(&hold(5, DriverSection::MapEnd))
                .unwrap()
                .commands
        ),
        [DriverCommand::EndMatch, DriverCommand::Continue(5)]
    );
}

#[test]
fn resumes_by_replaying_the_events() {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(reverse_cup_wasm()).unwrap();

    // Everything the driver handled in the first two rounds, checkpoints included.
    let mut events = vec![match_start(), hold(1, DriverSection::MapStart)];
    events.extend(round_with_give_ups(1, 2));
    events.extend(round_with_give_ups(2, 3));

    let mut driver = engine.instantiate(&module, SETTINGS).unwrap();
    play(&mut driver, &events);
    let mut resumed = engine.replay(&module, SETTINGS, &events).unwrap();

    // b only survived the second round thanks to the checkpoints, so both end the
    // match the same way.
    let last_round = final_round(3, 4);
    let reply = play(&mut resumed, &last_round);
    assert!(reply.commands.contains(&DriverCommand::EndMatch));
    assert_eq!(reply, play(&mut driver, &last_round));
}
