//! Runs the host with the rounds mode as a real wasm module. The modes test
//! their rules with simulations of their own.

mod common;

use common::{
    ROUNDS, SESSION, TIME_ATTACK, at, finish, forced_round_end, hold, match_start, no_warm_up,
    play, player, player_info, publishes_standings, race, ranking, round_start, start_line,
    way_point, without_published,
};
use tm_server_types::{
    event::{
        DriverHold, DriverPlayerResult, DriverRoundResult, DriverSection, Event, Pause,
        PlayLoopStart,
    },
    method::{DriverCommand, DriverFinishTimeout, DriverLayer, DriverRoundFormat, DriverStatus},
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
    // The round shows live from its start on.
    let started = driver.handle(&round_start(1)).unwrap();
    assert!(without_published(&started.commands).is_empty());
    assert!(driver.round().live);

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
    assert_eq!(driver.round().number, 1);

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
fn players_show_up_once_they_start_a_round() {
    let mut driver = rounds("");
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
    for player in ["a", "b"] {
        driver.handle(&player_info(player, player)).unwrap();
    }
    assert!(publishes_standings(
        &driver.handle(&start_line("b")).unwrap().commands
    ));
    driver.handle(&start_line("a")).unwrap();
    // Starting again, e.g. in time attack rounds, changes nothing.
    assert_eq!(
        driver.handle(&start_line("b")).unwrap(),
        DriverReply::default()
    );

    // Without a position until their first round, so the host lists them by name.
    assert_eq!(
        ranking(&driver),
        [
            ("a".into(), 0, DriverStatus::Playing, String::new(), 0, 0),
            ("b".into(), 0, DriverStatus::Playing, String::new(), 0, 0),
        ]
    );
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
    assert_eq!(
        without_published(&repeated.commands),
        without_published(&first.commands)
    );
    // The first reply might not have arrived, so everything is published again.
    assert!(publishes_standings(&repeated.commands));
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
fn a_resumed_driver_publishes_everything_again() {
    let mut driver = rounds("");
    driver
        .apply(&DriverInput::Players(vec![player("a", "Ant")]))
        .unwrap();
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();

    // Replies before the bridge restarted might not have reached the server.
    let published: Vec<&str> = driver
        .apply(&DriverInput::Resumed)
        .unwrap()
        .commands
        .iter()
        .map(|command| match command {
            DriverCommand::SetUsers(_) => "users",
            DriverCommand::SetStandings(_) => "standings",
            DriverCommand::SetRound(_) => "round",
            _ => "other",
        })
        .collect();
    assert_eq!(published, ["users", "standings", "round"]);
}

#[test]
fn a_hold_asked_again_after_its_reply_arrived_only_continues() {
    let mut driver = rounds(r#"{"rounds_per_map": 1}"#);
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));
    let first = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert!(first.commands.contains(&DriverCommand::EndMap));

    // The reply reached the server, so ending the map again would skip the next one.
    driver.apply(&DriverInput::Delivered).unwrap();
    let repeated = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(repeated.commands, [DriverCommand::Continue(2)]);
}

#[test]
fn forced_rounds_have_no_result() {
    let mut driver = rounds("");
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));

    let reply = driver.handle(&forced_round_end(2)).unwrap();
    assert_eq!(
        without_published(&reply.commands),
        [DriverCommand::Continue(2)]
    );
    assert_eq!(reply.round_result, None);
    // The round shows the last one that counted again, none yet.
    assert_eq!(driver.round().number, 0);
    assert!(!driver.round().live);
    assert!(driver.round().entries.is_empty());
}

/// Whether a reply publishes the round.
fn publishes_round(reply: &DriverReply) -> bool {
    reply
        .commands
        .iter()
        .any(|command| matches!(command, DriverCommand::SetRound(_)))
}

#[test]
fn the_round_shows_live_once_a_second_at_most() {
    let mut driver = rounds(r#"{"points_repartition": [10, 6]}"#);
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&round_start(1)).unwrap();
    let places = |driver: &Driver| -> Vec<(String, i32)> {
        driver
            .round()
            .entries
            .into_iter()
            .map(|entry| (entry.account_id, entry.score))
            .collect()
    };

    // The first start of the round shows right away.
    assert!(publishes_round(
        &driver.handle(&at(start_line("a"), 1_000)).unwrap()
    ));
    assert_eq!(places(&driver), [("a".to_string(), 0)]);
    // b starts within the second, which shows with the next change after it.
    assert!(!publishes_round(
        &driver.handle(&at(start_line("b"), 1_500)).unwrap()
    ));
    assert_eq!(places(&driver), [("a".to_string(), 0)]);
    let reply = driver.handle(&at(finish("b", 0, 30_000), 31_500)).unwrap();
    assert!(publishes_round(&reply));
    assert_eq!(
        places(&driver),
        [("b".to_string(), 10), ("a".to_string(), 0)]
    );
    assert!(!publishes_round(
        &driver.handle(&at(way_point("a", 0), 32_000)).unwrap()
    ));

    // The round end decides and shows the final round.
    let end = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert!(publishes_round(&end));
    assert!(!driver.round().live);
    assert_eq!(driver.round().number, 1);
    assert_eq!(
        places(&driver),
        [("b".to_string(), 10), ("a".to_string(), 0)]
    );
    // Only the round end awarded points.
    assert_eq!(driver.standings().entries[0].score, 10);

    // The next round starts empty, and a round that does not count shows the last
    // one again.
    driver.handle(&round_start(2)).unwrap();
    assert_eq!(driver.round().number, 2);
    assert!(driver.round().entries.is_empty());
    driver.handle(&at(start_line("a"), 40_000)).unwrap();
    assert_eq!(places(&driver), [("a".to_string(), 0)]);
    driver.handle(&forced_round_end(3)).unwrap();
    assert_eq!(driver.round().number, 1);
    assert_eq!(
        places(&driver),
        [("b".to_string(), 10), ("a".to_string(), 0)]
    );
}

#[test]
fn warmups_show_no_live_round() {
    let mut driver = rounds("");
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&Event::WarmupStart).unwrap();
    driver.handle(&round_start(1)).unwrap();
    driver.handle(&at(start_line("a"), 1_000)).unwrap();
    assert!(!driver.round().live);
}

#[test]
fn modes_bring_manialinks_in_place_of_selected_ones() {
    let selected = [
        DriverLayer::selected("standings", "<manialink/>"),
        DriverLayer::selected("scores-table", "<manialink/>"),
    ];
    // The rounds mode brings none.
    assert_eq!(rounds("").layers(&selected), selected);

    let time_attack = common::driver(TIME_ATTACK, "");
    let layers = time_attack.layers(&selected);
    let names: Vec<&str> = layers.iter().map(|layer| layer.name.as_str()).collect();
    assert_eq!(names, ["standings", "scores-table"]);
    // Its standings show the live round.
    assert!(layers[0].manialink.contains("Net_Driver_Round"));
    assert_ne!(layers[0], selected[0]);
    assert_eq!(layers[1], selected[1]);
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
    assert_eq!(driver.round().number, 0);
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

    // The modescript asks again and gets the same decisions without deciding twice,
    // and everything published again.
    let repeated = resumed.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(
        without_published(&repeated.commands),
        without_published(&decided.commands)
    );
    assert!(publishes_standings(&repeated.commands));
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

#[test]
fn a_pause_shows_in_the_standings() {
    let pause = |active| {
        Event::Pause(Pause {
            available: true,
            active,
        })
    };
    let mut driver = rounds("");
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();

    let paused = driver.handle(&pause(true)).unwrap();
    assert!(publishes_standings(&paused.commands));
    assert!(driver.standings().paused);
    driver.handle(&pause(false)).unwrap();
    assert!(!driver.standings().paused);

    // A restarted modescript is not paused.
    driver.handle(&pause(true)).unwrap();
    driver
        .handle(&Event::DriverHold(DriverHold {
            session: SESSION + 1,
            seq: 1,
            section: DriverSection::MapStart,
            forced: false,
        }))
        .unwrap();
    assert!(!driver.standings().paused);
}

/// Rounds mode settings with this points repartition and maps without end.
fn points(repartition: &str) -> String {
    format!(r#"{{"points_repartition": {repartition}, "rounds_per_map": 0}}"#)
}

#[test]
fn rounds_can_change_every_setting() {
    assert_eq!(
        rounds("").reloadable_settings(),
        [
            "points_repartition",
            "rounds_per_map",
            "maps_per_match",
            "points_limit",
            "finish_timeout",
            "warmup",
        ]
    );
}

#[test]
fn new_settings_wait_for_the_round_to_end() {
    let mut driver = rounds(&points("[10, 6]"));
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));

    let new = points("[3]");
    let reply = driver.apply(&DriverInput::Settings(new.clone())).unwrap();
    assert_eq!(reply, DriverReply::default());
    assert_eq!(driver.pending_settings(), Some(new.as_str()));

    // The round counts with the points it started with.
    let end = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(
        end.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", 10, 1)]
        })
    );
    assert_eq!(
        without_published(&end.commands),
        [DriverCommand::Continue(2)]
    );
    // The first input after the round takes the new ones, e.g. the bridge telling
    // that the reply arrived, and sends the round format again.
    let delivered = driver.apply(&DriverInput::Delivered).unwrap();
    assert!(matches!(
        without_published(&delivered.commands).first(),
        Some(DriverCommand::SetRoundFormat(_))
    ));
    assert_eq!(driver.settings(), new);
    assert_eq!(driver.pending_settings(), None);

    driver.handle(&round_start(2)).unwrap();
    play(&mut driver, &race("a", 30_000));
    let next = driver.handle(&hold(3, DriverSection::RoundEnd)).unwrap();
    assert_eq!(
        next.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", 3, 1)]
        })
    );
}

#[test]
fn new_settings_between_rounds_apply_at_once() {
    let mut driver = rounds(&points("[10]"));
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    let reply = driver.apply(&DriverInput::Settings(points("[4]"))).unwrap();
    assert!(matches!(
        without_published(&reply.commands).first(),
        Some(DriverCommand::SetRoundFormat(_))
    ));
    assert_eq!(driver.settings(), points("[4]"));

    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));
    let end = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(
        end.round_result,
        Some(DriverRoundResult {
            results: vec![result("a", 4, 1)]
        })
    );
}

#[test]
fn rejected_settings_change_nothing() {
    let mut driver = rounds(&points("[10]"));
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    // Between rounds and during one.
    let unknown = DriverInput::Settings(r#"{"points": [3]}"#.into());
    assert!(driver.apply(&unknown).unwrap().rejected.is_some());
    driver.handle(&round_start(1)).unwrap();
    assert!(driver.apply(&unknown).unwrap().rejected.is_some());
    assert_eq!(driver.settings(), points("[10]"));
    assert_eq!(driver.pending_settings(), None);
}

#[test]
fn a_replay_takes_the_new_settings_again() {
    let (engine, module) = common::compile(ROUNDS);
    let settings = points("[10]");
    let mut inputs = common::inputs(&[hold(1, DriverSection::MapStart), round_start(1)]);
    inputs.extend(common::inputs(&race("a", 30_000)));
    inputs.push(DriverInput::Settings(points("[3]")));
    inputs.extend(common::inputs(&[
        hold(2, DriverSection::RoundEnd),
        round_start(2),
    ]));
    inputs.extend(common::inputs(&race("b", 30_000)));
    inputs.extend(common::inputs(&[hold(3, DriverSection::RoundEnd)]));

    let mut driver = engine.instantiate(&module, &settings).unwrap();
    for input in &inputs {
        driver.apply(input).unwrap();
    }
    let replayed = engine.replay(&module, &settings, &inputs).unwrap();
    assert_eq!(replayed.standings(), driver.standings());
    assert_eq!(replayed.settings(), points("[3]"));
}
