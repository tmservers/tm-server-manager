use tm_mode_driver_sdk::{
    CanSpawn, Command, FinishTimeout, Race, Racer, RoundFormat, Status, Tone, WarmUp,
    testing::TestHost,
};

use super::*;

/// A script which only defines `step` with this body.
fn stepping(body: &str) -> String {
    format!("fn step(cx, standings, event) {{ {body} }}")
}

fn start(source: &str, settings: &str) -> Box<dyn ModeInstance> {
    ScriptMode::new(source)
        .expect("the script compiles")
        .start(settings)
        .expect("starting does not fault")
        .expect("the settings are valid")
}

fn rejection(source: &str, settings: &str) -> String {
    ScriptMode::new(source)
        .expect("the script compiles")
        .start(settings)
        .expect("starting does not fault")
        .err()
        .expect("the settings are rejected")
}

fn step(mode: &mut dyn ModeInstance, standings: &Standings, event: Event) -> Decision {
    mode.step(&TestHost::default(), standings, event)
        .expect("the step does not fault")
}

fn finish(account_id: &str, checkpoints: &[u32]) -> Racer {
    Racer {
        account_id: account_id.into(),
        races: vec![Race {
            checkpoints: checkpoints.to_vec(),
            finished: true,
            laps: 1,
        }],
    }
}

fn round_end(racers: Vec<Racer>) -> RoundEnd {
    RoundEnd {
        invalid: false,
        present: racers.iter().map(|r| r.account_id.clone()).collect(),
        racers,
    }
}

#[test]
fn a_script_needs_a_step_function_with_three_arguments() {
    assert_eq!(
        ScriptMode::new("fn name() { \"x\" }").unwrap_err(),
        "The script has to define `fn step(cx, standings, event)`."
    );
    assert_eq!(
        ScriptMode::new("fn step(standings, event) { [] }").unwrap_err(),
        "`step` of the script has to take 3 arguments, not 2."
    );
    assert_eq!(
        ScriptMode::new(&format!("{} fn validate() {{ }}", stepping("[]"))).unwrap_err(),
        "`validate` of the script has to take 1 arguments, not 0."
    );
    assert!(
        ScriptMode::new("fn step(cx, standings, event) { [ }")
            .unwrap_err()
            .starts_with("The script does not compile:")
    );
}

#[test]
fn a_script_changes_the_standings_it_gets_and_returns_commands() {
    let mut mode = start(
        &stepping(
            r#"
            standings.award("a", 3);
            standings.award("b", 5);
            standings.set_status("a", "eliminated");
            standings.set_label("b", "Winner", "positive");
            standings.set_position("b", 1);
            standings.info = `${standings.entries.len()} players`;
            standings.rounds_on_map += 1;
            [single_attempt(#{ milliseconds: 5000 }), warm_up(#{ rounds: 2 }), can_spawn("a", false), chat("gg"), end_map()]
            "#,
        ),
        "",
    );
    let before = Standings::default();

    let decision = step(mode.as_mut(), &before, Event::MapStart);

    // The standings the host holds only change with what the script returns.
    assert_eq!(before, Standings::default());
    let entries: Vec<_> = decision
        .standings
        .entries
        .iter()
        .map(|e| (e.account_id.as_str(), e.score, e.status, e.position))
        .collect();
    assert_eq!(
        entries,
        [
            ("a", 3, Status::Eliminated, 0),
            ("b", 5, Status::Playing, 1)
        ]
    );
    let label = decision.standings.entries[1].label.clone().unwrap();
    assert_eq!(
        (label.text.as_str(), label.tone),
        ("Winner", Tone::Positive)
    );
    // Awarded points also count for the round.
    assert_eq!(decision.standings.round.len(), 2);
    assert_eq!(decision.standings.info, "2 players");
    assert_eq!(decision.standings.rounds_on_map, 1);
    assert_eq!(
        decision.commands,
        [
            Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                5000
            ))),
            Command::SetWarmUp(WarmUp {
                rounds: 2,
                duration_ms: 0,
                timeout_ms: None
            }),
            Command::SetCanSpawn(CanSpawn {
                account_id: "a".into(),
                can_spawn: false
            }),
            Command::ChatMessage("gg".into()),
            Command::EndMap,
        ]
    );
}

#[test]
fn a_script_ranks_a_round_without_ties_like_the_built_in_modes() {
    let mut mode = start(
        &stepping(
            r#"
            if event.kind != "round_end" { return []; }
            let round = event.round;
            for account in round.finishers("last") {
                standings.award(account, 10);
            }
            standings.rank(round, "last");
            standings.place_round(round, "last");
            []
            "#,
        ),
        "",
    );
    // Everyone finishes in 30 seconds and gets the same points.
    let round = round_end(vec![
        finish("c", &[10_000, 30_000]),
        finish("b", &[10_000, 30_000]),
        finish("a", &[11_000, 30_000]),
    ]);

    let decision = step(mode.as_mut(), &Standings::default(), Event::RoundEnd(round));

    // The checkpoint before the finish decides, then the account.
    let ranked = |entries: &[Entry]| -> Vec<(String, u32)> {
        entries
            .iter()
            .map(|e| (e.account_id.clone(), e.position))
            .collect()
    };
    let expected = [
        ("b".to_string(), 1),
        ("c".to_string(), 2),
        ("a".to_string(), 3),
    ];
    assert_eq!(ranked(&decision.standings.entries), expected);
    assert_eq!(ranked(&decision.standings.round), expected);
    assert_eq!(decision.standings.round[0].time, Some(30_000));
}

#[test]
fn a_script_reads_its_settings_and_what_the_host_knows() {
    let host = TestHost {
        map_count: 4,
        users: [(
            "a".to_string(),
            User {
                account_id: "a".into(),
                name: "Ant".into(),
                ..User::default()
            },
        )]
        .into(),
    };
    let source = format!(
        "fn defaults() {{ #{{ greeting: \"hi\", maps_per_match: () }} }} {}",
        stepping(
            r#"
            cx.log("stepped");
            print("printed");
            standings.info = `${cx.settings.greeting} ${cx.name(event.account_id)}, ${cx.name("b")} of ${cx.maps_per_match(cx.settings.maps_per_match)} / ${cx.map_count}`;
            []
            "#
        )
    );
    let mut mode = start(&source, r#"{"greeting": "hello"}"#);
    let mut standings = Standings::default();
    standings.join("b");

    let decision = mode
        .step(&host, &standings, Event::RaceStart("a".into()))
        .unwrap();

    // b is in the standings but the host does not know the name yet.
    assert_eq!(decision.standings.info, "hello Ant, b of 4 / 4");
}

#[test]
fn settings_are_checked_against_the_defaults_and_by_the_script() {
    let source = format!(
        "fn defaults() {{ #{{ rounds: 5 }} }}
         fn validate(settings) {{ if settings.rounds < 1 {{ throw \"rounds has to be 1 at least\"; }} }}
         {}",
        stepping("[]")
    );

    assert_eq!(
        rejection(&source, r#"{"round": 3}"#),
        "Invalid settings: unknown setting `round`"
    );
    assert_eq!(
        rejection(&source, r#"{"rounds": 0}"#),
        "Invalid settings: rounds has to be 1 at least"
    );
    assert_eq!(
        rejection(&source, "not json").split(':').next(),
        Some("Invalid settings")
    );
    assert!(
        ScriptMode::new(source)
            .unwrap()
            .start(r#"{"rounds": 1}"#)
            .unwrap()
            .is_ok()
    );
}

#[test]
fn only_reloadable_settings_change_during_a_match() {
    let source = format!(
        "fn defaults() {{ #{{ points: 1, lives: 3 }} }}
         fn reloadable() {{ [\"points\"] }}
         {}",
        stepping("standings.award(\"a\", cx.settings.points); []")
    );
    let mut mode = start(&source, "");
    let award = |mode: &mut dyn ModeInstance| {
        step(mode, &Standings::default(), Event::RoundStart)
            .standings
            .entries[0]
            .score
    };
    assert_eq!(mode.reloadable_settings(), ["points"]);

    // Checking takes nothing.
    assert_eq!(mode.check_settings(r#"{"points": 7}"#), Ok(Ok(())));
    assert_eq!(award(mode.as_mut()), 1);
    assert_eq!(
        mode.change_settings(r#"{"points": 7, "lives": 2}"#),
        Ok(Err("lives cannot change during a match".into()))
    );
    assert_eq!(award(mode.as_mut()), 1);
    assert_eq!(mode.change_settings(r#"{"points": 7}"#), Ok(Ok(())));
    assert_eq!(award(mode.as_mut()), 7);
}

#[test]
fn a_script_which_never_ends_runs_out_of_its_budget() {
    let mut mode = start(&stepping("loop { }"), "");

    let fault = mode
        .step(
            &TestHost::default(),
            &Standings::default(),
            Event::RoundStart,
        )
        .unwrap_err();

    assert!(fault.0.contains("Too many operations"), "{fault}");
    assert_eq!(mode.budget(), Some(BUDGET));
}

#[test]
fn a_call_costs_the_same_whenever_it_runs() {
    let mut mode = start(
        &stepping("let sum = 0; for i in 0..100 { sum += i; } standings.rounds_on_map = sum; []"),
        "",
    );
    let mut cost = || {
        let decision = step(mode.as_mut(), &Standings::default(), Event::RoundStart);
        assert_eq!(decision.standings.rounds_on_map, 4950);
        mode.last_cost().unwrap()
    };

    let first = cost();

    assert!(first > 100 && first < 1_000, "{first}");
    // Every call starts with a budget of its own.
    assert_eq!(cost(), first);
}

#[test]
fn what_a_script_gets_wrong_is_a_fault() {
    let fault = |body: &str| {
        start(&stepping(body), "")
            .step(
                &TestHost::default(),
                &Standings::default(),
                Event::RoundStart,
            )
            .unwrap_err()
            .0
    };

    assert_eq!(
        fault("[1]"),
        "step of the script returned a i32 instead of a command."
    );
    assert!(fault("standings.set_score(\"a\", 1); []").contains("a is not part of the standings"));
    assert!(
        fault("standings.rank(event.round, \"last\"); []")
            .contains("A round_start event has no round.")
    );
    assert!(
        fault("standings.join(\"a\"); standings.set_status(\"a\", \"out\"); []")
            .contains("A status is")
    );
    assert!(fault("cx.settings.typo; []").contains("typo"));
}

#[test]
fn the_live_round_of_a_script_cannot_change_the_match() {
    let source = format!(
        "{}
         fn live_round(cx, standings, round) {{
             standings.award(\"a\", 100);
             standings.placed(round, \"last\", #{{ a: 10 }})
         }}",
        stepping("[]")
    );
    let mut mode = start(&source, "");
    let mut standings = Standings::default();
    standings.join("a");
    standings.join("b");
    let races = round_end(vec![finish("b", &[31_000]), finish("a", &[30_000])]);

    let live = mode
        .live_round(&TestHost::default(), &standings, &races)
        .unwrap();

    let placed: Vec<_> = live
        .iter()
        .map(|e| (e.account_id.as_str(), e.position, e.score, e.time))
        .collect();
    assert_eq!(
        placed,
        [("a", 1, 10, Some(30_000)), ("b", 2, 0, Some(31_000))]
    );
    assert_eq!(standings.player("a").map(|a| a.score), Some(0));
    // Without the function the round only shows once it ended.
    let mut plain = start(&stepping("[]"), "");
    assert_eq!(
        plain.live_round(&TestHost::default(), &standings, &races),
        Ok(Vec::new())
    );
}
