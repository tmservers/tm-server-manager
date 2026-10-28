//! Runs the first-party rounds mode through the host as a real wasm module.

mod common;

use common::{
    ROUNDS, SESSION, finish, forced_round_end, hold, match_start, no_warm_up, play, player,
    player_info, publishes_standings, race, ranking, round_start, without_published,
};
use tm_server_types::{
    event::{
        DriverHold, DriverPlayerResult, DriverRoundResult, DriverSection, Event, PlayLoopStart,
    },
    method::{DriverCommand, DriverFinishTimeout, DriverRoundFormat, DriverStatus},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverError, DriverInput, DriverLimits, DriverReply};

fn rounds(settings: &str) -> Driver {
    common::driver(ROUNDS, settings)
}

/// The settings of a match that ends once a player has more than 25 points.
const POINTS_LIMIT: &str = r#"{"rounds_per_map": 0, "points_limit": 25}"#;

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

    // A new match starts with empty standings.
    let reply = driver.handle(&match_start()).unwrap();
    assert!(publishes_standings(&reply.commands));
    assert!(without_published(&reply.commands).is_empty());

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
    assert_eq!(
        driver.handle(&round_start(1)).unwrap(),
        DriverReply::default()
    );

    // b has the better best time but rounds only count the last race. c is on the
    // server without racing.
    play(&mut driver, &race("a", 30_000));
    play(&mut driver, &race("b", 29_000));
    play(&mut driver, &race("b", 31_000));
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
            results: vec![result("a", 10, 1), result("b", 6, 2)]
        })
    );
    assert_eq!(driver.last_round().number, 1);

    assert_eq!(
        without_published(
            &driver
                .handle(&hold(3, DriverSection::MapEnd))
                .unwrap()
                .commands
        ),
        [DriverCommand::EndMatch, DriverCommand::Continue(3)]
    );

    // The host keeps the standings, the mode crowned the leader.
    assert_eq!(
        ranking(&driver),
        [
            (
                "a".into(),
                1,
                DriverStatus::Playing,
                "Winner".into(),
                10,
                10
            ),
            ("b".into(), 2, DriverStatus::Playing, String::new(), 6, 6),
        ]
    );
    assert_eq!(driver.standings().title, "Rounds");
    assert_eq!(driver.standings().maps_played, 1);
}

#[test]
fn repeated_holds_are_decided_once() {
    // The match ends once a player has more than 25 points.
    let mut driver = rounds(POINTS_LIMIT);
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();

    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));
    let first = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    let repeated = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(repeated.commands, first.commands);
    // The result was already reported with the first reply.
    assert_eq!(repeated.round_result, None);

    // a has 20 points now. With the repeated hold counted it would be 30.
    driver.handle(&round_start(2)).unwrap();
    play(&mut driver, &race("a", 30_000));
    assert_eq!(
        without_published(
            &driver
                .handle(&hold(3, DriverSection::RoundEnd))
                .unwrap()
                .commands
        ),
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
    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));

    let reply = driver.handle(&forced_round_end(2)).unwrap();
    assert_eq!(reply.commands, [DriverCommand::Continue(2)]);
    assert_eq!(reply.round_result, None);
    assert_eq!(driver.last_round().number, 0);
}

#[test]
fn rounds_without_their_start_do_not_count() {
    let mut driver = rounds("");
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    // The driver missed the start of the round, e.g. because the bridge restarted.
    driver.handle(&finish("a", 0, 30_000)).unwrap();

    let reply = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(reply.round_result, None);
    assert!(driver.standings().entries.is_empty());
    assert_eq!(driver.last_round().number, 0);
}

#[test]
fn publishes_the_players_it_knows() {
    let mut driver = rounds("");
    let reply = driver.handle(&player_info("a", "$f00A")).unwrap();
    assert!(
        reply
            .commands
            .iter()
            .any(|c| matches!(c, DriverCommand::SetUsers(_)))
    );
    let users = driver.users().users;
    assert_eq!(users.len(), 1);
    assert_eq!(
        (users[0].account_id.as_str(), users[0].name.as_str()),
        ("a", "$f00A")
    );

    // Nothing changed, so nothing is published.
    assert!(
        driver
            .handle(&player_info("a", "$f00A"))
            .unwrap()
            .commands
            .is_empty()
    );
    // The server shows up as a player too.
    let server = Event::PlayerInfoChanged(common::server());
    assert!(driver.handle(&server).unwrap().commands.is_empty());
    assert_eq!(driver.users().users.len(), 1);
}

#[test]
fn players_already_on_the_server_are_published_once() {
    let mut driver = rounds("");
    let players = DriverInput::Players(vec![player("a", "A"), player("b", "B"), common::server()]);
    let commands = driver.apply(&players).unwrap().commands;
    let users = commands
        .iter()
        .filter(|c| matches!(c, DriverCommand::SetUsers(_)))
        .count();
    assert_eq!(users, 1);
    assert_eq!(driver.users().users.len(), 2);
    // Nothing changed, so nothing is published.
    let again = DriverInput::Players(vec![player("a", "A")]);
    assert!(driver.apply(&again).unwrap().commands.is_empty());
}

#[test]
fn a_new_match_publishes_everything_again() {
    let mut driver = rounds("");
    driver.handle(&player_info("a", "A")).unwrap();
    driver.handle(&match_start()).unwrap();
    let reply = driver.handle(&match_start()).unwrap();
    let published: Vec<&str> = reply
        .commands
        .iter()
        .map(|c| match c {
            DriverCommand::SetUsers(_) => "users",
            DriverCommand::SetStandings(_) => "standings",
            DriverCommand::SetRound(_) => "round",
            _ => "other",
        })
        .collect();
    assert_eq!(published, ["users", "standings", "round"]);
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
fn resumes_by_replaying_the_events() {
    let (engine, module) = common::compile(ROUNDS);
    let mut driver = engine.instantiate(&module, POINTS_LIMIT).unwrap();

    // Everything the driver handled so far, like the event stream of the match.
    let mut events = vec![
        match_start(),
        hold(1, DriverSection::MapStart),
        round_start(1),
    ];
    events.extend(race("a", 30_000));
    events.push(hold(2, DriverSection::RoundEnd));
    let mut decided = DriverReply::default();
    for event in &events {
        decided = driver.handle(event).unwrap();
    }

    // The bridge went away before the server got the reply to the last hold.
    drop(driver);
    let mut resumed = engine
        .replay(&module, POINTS_LIMIT, &common::inputs(&events))
        .unwrap();

    // The modescript asks again and gets the same commands without deciding twice.
    let repeated = resumed.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(repeated.commands, decided.commands);
    assert_eq!(repeated.round_result, None);

    // The points of the first round are back: 10 + 10 + 10 reaches the limit in the third round.
    resumed.handle(&round_start(2)).unwrap();
    play(&mut resumed, &race("a", 30_000));
    resumed.handle(&hold(3, DriverSection::RoundEnd)).unwrap();
    resumed.handle(&round_start(3)).unwrap();
    play(&mut resumed, &race("a", 30_000));
    assert_eq!(
        without_published(
            &resumed
                .handle(&hold(4, DriverSection::RoundEnd))
                .unwrap()
                .commands
        ),
        [DriverCommand::EndMatch, DriverCommand::Continue(4)]
    );
}

#[test]
fn replays_are_deterministic() {
    let (engine, module) = common::compile(ROUNDS);
    let mut events = vec![
        match_start(),
        player_info("c", "c"),
        hold(1, DriverSection::MapStart),
        round_start(1),
    ];
    events.extend(race("a", 30_000));
    events.extend(race("b", 29_000));
    events.extend([hold(2, DriverSection::RoundEnd), round_start(2)]);
    events.extend(race("a", 28_000));
    events.extend(race("c", 31_000));
    events.extend([
        hold(3, DriverSection::RoundEnd),
        hold(4, DriverSection::MapEnd),
    ]);

    let replies = |events: &[Event]| {
        let mut driver = engine.instantiate(&module, POINTS_LIMIT).unwrap();
        events
            .iter()
            .map(|event| driver.handle(event).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(replies(&events), replies(&events));
}

#[test]
fn ignores_unrelated_callbacks() {
    let mut driver = rounds("");
    let play_loop = Event::PlayLoopStart(PlayLoopStart {
        count: 1,
        valid: 1,
        time: 0,
    });
    assert!(!Driver::handles(&play_loop));
    assert_eq!(driver.handle(&play_loop).unwrap(), DriverReply::default());
}

#[test]
fn rejects_invalid_settings() {
    let (engine, module) = common::compile(ROUNDS);
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
    let module = engine.compile(common::mode(ROUNDS)).unwrap();
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
