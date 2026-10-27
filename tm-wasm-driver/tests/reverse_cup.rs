//! Runs the first-party reverse cup mode through the host as a real wasm module.

mod common;

use common::{
    REVERSE_CUP, finish, hold, match_start, no_warm_up, play, player_info, publishes_standings,
    ranking, round_start, start_line, way_point, without_published,
};
use tm_server_types::{
    event::{DriverPlayerResult, DriverRoundResult, DriverSection, Event},
    method::{DriverCanSpawn, DriverCommand, DriverFinishTimeout, DriverRoundFormat, DriverStatus},
};
use tm_wasm_driver::DriverReply;

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
/// s is on the server and only watches.
fn round_with_give_ups(round: u32, seq: u32) -> Vec<Event> {
    vec![
        player_info("s", "s"),
        round_start(round),
        start_line("a"),
        start_line("b"),
        start_line("c"),
        way_point("a", 0),
        way_point("b", 0),
        way_point("c", 0),
        way_point("a", 1),
        way_point("b", 1),
        finish("a", 2, 30_000),
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
        finish("a", 1, 30_000),
        finish("b", 1, 32_000),
        hold(seq, DriverSection::RoundEnd),
    ]
}

#[test]
fn plays_a_match() {
    let mut driver = common::driver(REVERSE_CUP, SETTINGS);

    driver.handle(&match_start()).unwrap();
    assert_eq!(
        without_published(
            &driver
                .handle(&hold(1, DriverSection::MapStart))
                .unwrap()
                .commands
        ),
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt(
                DriverFinishTimeout::Automatic
            )),
            no_warm_up(),
            DriverCommand::Continue(1)
        ]
    );

    // The players who start the first round take part. Three players lose 2, 6
    // and 9 points, giving up costs 20 and puts b and c in last chance.
    let reply = play(&mut driver, &round_with_give_ups(1, 2));
    assert_eq!(
        without_published(&reply.commands),
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
        without_published(&reply.commands),
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
        without_published(&reply.commands),
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
            ("a".into(), 1, DriverStatus::Playing, "Winner".into(), 2, -4),
            (
                "b".into(),
                2,
                DriverStatus::Eliminated,
                String::new(),
                0,
                -8
            ),
            ("c".into(), 3, DriverStatus::Eliminated, String::new(), 0, 0),
            // Watched without taking part.
            ("s".into(), 0, DriverStatus::Spectator, String::new(), 0, 0),
        ]
    );
    assert_eq!(driver.standings().info, "1 left, 3 / 5 rounds");

    assert_eq!(
        without_published(
            &driver
                .handle(&hold(5, DriverSection::MapEnd))
                .unwrap()
                .commands
        ),
        [DriverCommand::EndMatch, DriverCommand::Continue(5)]
    );
}

#[test]
fn standings_show_the_players_once_they_start() {
    let mut driver = common::driver(REVERSE_CUP, SETTINGS);

    driver.handle(&match_start()).unwrap();
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    // Warmup races happen outside of rounds, even if a warmup round looked like one.
    driver.handle(&Event::WarmupStart).unwrap();
    driver.handle(&start_line("a")).unwrap();
    driver.handle(&round_start(1)).unwrap();
    driver.handle(&start_line("b")).unwrap();
    driver.handle(&Event::WarmupEnd).unwrap();
    assert!(driver.standings().entries.is_empty());

    driver.handle(&round_start(1)).unwrap();
    for player in ["a", "b", "s"] {
        driver.handle(&player_info(player, player)).unwrap();
    }
    assert!(publishes_standings(
        &driver.handle(&start_line("a")).unwrap().commands
    ));
    driver.handle(&start_line("b")).unwrap();
    // Starting again, e.g. in time attack rounds, changes nothing.
    assert_eq!(
        driver.handle(&start_line("a")).unwrap(),
        DriverReply::default()
    );

    assert_eq!(
        ranking(&driver),
        [
            ("a".into(), 1, DriverStatus::Playing, String::new(), 10, 0),
            ("b".into(), 1, DriverStatus::Playing, String::new(), 10, 0),
        ]
    );
    assert_eq!(driver.standings().info, "2 left, 0 / 5 rounds");
}

#[test]
fn resumes_by_replaying_the_events() {
    let (engine, module) = common::compile(REVERSE_CUP);

    // Everything the driver handled in the first two rounds, checkpoints included.
    let mut events = vec![match_start(), hold(1, DriverSection::MapStart)];
    events.extend(round_with_give_ups(1, 2));
    events.extend(round_with_give_ups(2, 3));

    let mut driver = engine.instantiate(&module, SETTINGS).unwrap();
    play(&mut driver, &events);
    let mut resumed = engine.replay(&module, SETTINGS, &[], &events).unwrap();

    // b only survived the second round thanks to the checkpoints, so both end the
    // match the same way.
    let last_round = final_round(3, 4);
    let reply = play(&mut resumed, &last_round);
    assert!(reply.commands.contains(&DriverCommand::EndMatch));
    assert_eq!(reply, play(&mut driver, &last_round));
}
