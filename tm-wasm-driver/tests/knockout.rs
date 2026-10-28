//! Runs the first-party knockout mode through the host as a real wasm module.

mod common;

use common::{
    KNOCKOUT, finish, hold, match_start, no_warm_up, play, player, player_info, race, ranking,
    round_start, start_line, way_point, without_published,
};
use tm_server_types::{
    event::{DriverPlayerResult, DriverRoundResult, DriverSection, Event},
    method::{DriverCanSpawn, DriverCommand, DriverFinishTimeout, DriverRoundFormat, DriverStatus},
};
use tm_wasm_driver::DriverInput;

fn result(account_id: &str, position: u32, points: i32) -> DriverPlayerResult {
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

/// A round in which the players finish in this order, a second apart.
fn round(round: u32, seq: u32, players: &[&str]) -> Vec<Event> {
    let mut events = vec![round_start(round)];
    for (index, player) in players.iter().enumerate() {
        events.extend(race(player, 30_000 + 1_000 * index as u32));
    }
    events.push(hold(seq, DriverSection::RoundEnd));
    events
}

/// A round in which b finishes before a and c gives up after the first checkpoint.
fn round_with_a_give_up(round: u32, seq: u32) -> Vec<Event> {
    vec![
        round_start(round),
        start_line("a"),
        start_line("b"),
        start_line("c"),
        way_point("a", 0),
        way_point("b", 0),
        way_point("c", 0),
        finish("b", 1, 30_000),
        finish("a", 1, 31_000),
        hold(seq, DriverSection::RoundEnd),
    ]
}

/// A player passed a checkpoint with these race times at it and at every
/// checkpoint before it, like the server sends them.
fn way_point_at(account_id: &str, checkpoints: &[u32]) -> Event {
    let Event::WayPoint(mut way_point) = way_point(account_id, checkpoints.len() as u32 - 1) else {
        unreachable!()
    };
    way_point.racetime = checkpoints[checkpoints.len() - 1];
    way_point.current_race_checkpoints = checkpoints.to_vec();
    Event::WayPoint(way_point)
}

#[test]
fn plays_a_match() {
    let mut driver = common::driver(KNOCKOUT, "");

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
                DriverFinishTimeout::Milliseconds(5_000)
            )),
            no_warm_up(),
            DriverCommand::Continue(1)
        ]
    );

    // Nobody is eliminated in the first round. s only watches.
    driver.handle(&player_info("s", "s")).unwrap();
    let reply = play(&mut driver, &round(1, 2, &["c", "a", "b"]));
    assert_eq!(
        without_published(&reply.commands),
        [DriverCommand::Continue(2)]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![result("c", 1, 0), result("a", 2, 0), result("b", 3, 0)]
        })
    );
    assert_eq!(driver.standings().info, "Round 2 / 3, 3 left, 1 K.O.");

    // Giving up costs c the only life. Players cannot join anymore, so s cannot spawn.
    let reply = play(&mut driver, &round_with_a_give_up(2, 3));
    assert_eq!(
        without_published(&reply.commands),
        [
            DriverCommand::ChatMessage("$f90$i$<c$> takes 3rd place!".into()),
            cannot_spawn("c"),
            cannot_spawn("s"),
            DriverCommand::Continue(3)
        ]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![result("b", 1, 0), result("a", 2, 0), result("c", 3, -1)]
        })
    );

    // The last two players decide the match.
    let reply = play(&mut driver, &round(3, 4, &["a", "b"]));
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
        ranking(&driver),
        [
            ("a".into(), 1, DriverStatus::Playing, "Winner".into(), 1, 0),
            (
                "b".into(),
                2,
                DriverStatus::Eliminated,
                "K.O.".into(),
                0,
                -1
            ),
            ("c".into(), 3, DriverStatus::Eliminated, "K.O.".into(), 0, 0),
            // Watched without taking part.
            ("s".into(), 0, DriverStatus::Spectator, String::new(), 0, 0),
        ]
    );
    assert_eq!(driver.standings().info, "Match over");

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
fn players_in_the_match_show_their_last_race() {
    let mut driver = common::driver(KNOCKOUT, "");
    driver.handle(&match_start()).unwrap();
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    play(&mut driver, &round(1, 2, &["b", "a"]));

    assert_eq!(
        ranking(&driver),
        [
            (
                "b".into(),
                1,
                DriverStatus::Playing,
                "0:30.000".into(),
                1,
                0
            ),
            (
                "a".into(),
                2,
                DriverStatus::Playing,
                "0:31.000".into(),
                1,
                0
            ),
        ]
    );
}

#[test]
fn players_lose_lives_before_they_are_eliminated() {
    let mut driver = common::driver(KNOCKOUT, r#"{"lives": 2}"#);
    driver.handle(&match_start()).unwrap();
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    play(&mut driver, &round(1, 2, &["a", "b"]));

    // b loses a life and keeps racing.
    let reply = play(&mut driver, &round(2, 3, &["a", "b"]));
    assert_eq!(
        without_published(&reply.commands),
        [DriverCommand::Continue(3)]
    );
    assert_eq!(
        reply.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", 1, 0), result("b", 2, -1)]
        })
    );
    // Full and lost hearts of the game font.
    assert_eq!(
        ranking(&driver),
        [
            (
                "a".into(),
                1,
                DriverStatus::Playing,
                "\u{e96c}\u{e96c}".into(),
                2,
                0
            ),
            (
                "b".into(),
                2,
                DriverStatus::Playing,
                "\u{e96f}\u{e96c}".into(),
                1,
                -1
            ),
        ]
    );
    assert_eq!(driver.standings().info, "Round 3, 2 left, 1 loses a life");

    // The last life is gone.
    let reply = play(&mut driver, &round(3, 4, &["a", "b"]));
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
}

#[test]
fn players_who_got_equally_far_go_by_their_checkpoint_times() {
    let mut driver = common::driver(KNOCKOUT, "");
    driver.handle(&match_start()).unwrap();
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    play(&mut driver, &round(1, 2, &["a", "b", "c"]));

    // b and c give up after the second checkpoint, c got there first.
    let reply = play(
        &mut driver,
        &[
            round_start(2),
            start_line("a"),
            start_line("b"),
            start_line("c"),
            way_point_at("b", &[10_000, 21_000]),
            way_point_at("c", &[10_500, 20_000]),
            finish("a", 2, 30_000),
            hold(3, DriverSection::RoundEnd),
        ],
    );
    assert_eq!(
        without_published(&reply.commands),
        [
            DriverCommand::ChatMessage("$f90$i$<c$> takes 2nd place!".into()),
            cannot_spawn("c"),
            DriverCommand::ChatMessage("$f90$i$<b$> takes 3rd place!".into()),
            cannot_spawn("b"),
            DriverCommand::ChatMessage("$<a$> wins the match!".into()),
            DriverCommand::EndMatch,
            DriverCommand::Continue(3)
        ]
    );
}

#[test]
fn resumes_by_replaying_the_events() {
    let (engine, module) = common::compile(KNOCKOUT);

    // Everything the driver handled in the first two rounds, checkpoints included.
    let mut events = vec![
        match_start(),
        hold(1, DriverSection::MapStart),
        player_info("s", "s"),
    ];
    events.extend(round(1, 2, &["c", "a", "b"]));
    events.extend(round_with_a_give_up(2, 3));

    let mut driver = engine.instantiate(&module, "").unwrap();
    play(&mut driver, &events);
    let mut resumed = engine
        .replay(&module, "", &common::inputs(&events))
        .unwrap();

    // Both know who is left and end the match the same way.
    let last_round = round(3, 4, &["a", "b"]);
    let reply = play(&mut resumed, &last_round);
    assert!(reply.commands.contains(&DriverCommand::EndMatch));
    assert_eq!(reply, play(&mut driver, &last_round));
}

#[test]
fn resumes_with_the_players_already_on_the_server() {
    let (engine, module) = common::compile(KNOCKOUT);

    // s was on the server before the mode loaded and only watches, so no event tells
    // about s.
    let mut inputs = vec![DriverInput::Players(vec![player("s", "s")])];
    inputs.extend(common::inputs(&[
        match_start(),
        hold(1, DriverSection::MapStart),
    ]));
    inputs.extend(common::inputs(&round(1, 2, &["c", "a", "b"])));
    inputs.extend(common::inputs(&round_with_a_give_up(2, 3)));

    let mut driver = engine.instantiate(&module, "").unwrap();
    for input in &inputs {
        driver.apply(input).unwrap();
    }
    let resumed = engine.replay(&module, "", &inputs).unwrap();

    // s cannot spawn since the second round and watches, also after the replay.
    let standings = resumed.standings();
    let watcher = standings.entries.last().unwrap();
    assert_eq!(
        (watcher.account_id.as_str(), watcher.status),
        ("s", DriverStatus::Spectator)
    );
    assert_eq!(ranking(&resumed), ranking(&driver));
}
