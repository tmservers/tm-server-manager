//! What players do in manialinks reaches the mode, what the mode gives the manialinks
//! goes out and comes back to a modescript that lost it. A small mode of votes stands
//! in for a real one.

mod common;

use common::match_start;
use serde::{Deserialize, Serialize};
use tm_mode_driver::{
    Driver, DriverError, DriverInput, DriverLimits, DriverUiAction, RecordedDriver, RecordingDriver,
};
use tm_mode_driver_sdk::{Command, Event, Host, Mode, Native, Standings, UiData, UiScope, UiValue};
use tm_server_types::{
    config::UiValue as Parsed,
    event::DriverRecord,
    method::{DriverCommand, DriverNetUiData},
};

/// `join` puts a player into the standings and tells them, `score` gives them the
/// points it carries, `team` tells the players of the team, `reset` removes the key
/// of everyone.
struct Votes;

#[derive(Default, Serialize, Deserialize)]
struct NoSettings {}

impl Mode for Votes {
    const NAME: &'static str = "Votes";
    type Settings = NoSettings;

    fn init(_: NoSettings) -> Result<Self, String> {
        Ok(Votes)
    }

    fn step(&self, _: &dyn Host, standings: &mut Standings, event: Event) -> Vec<Command> {
        let data = |scope, key: &str, value: &str| {
            Command::SetUiData(UiData {
                scope,
                key: key.into(),
                value: value.into(),
            })
        };
        if let Event::MatchStart = event {
            return vec![data(UiScope::All, "phase", "start")];
        }
        let Event::UiAction(action) = event else {
            return Vec::new();
        };
        match (action.event.as_str(), action.args.as_slice()) {
            ("join", []) => {
                standings.join(&action.account_id);
                vec![
                    data(UiScope::Account(action.account_id.clone()), "joined", "1"),
                    data(UiScope::All, "joined-in", &action.layer),
                ]
            }
            ("score", [UiValue::Int(points), UiValue::User(target)]) => {
                standings.join(target);
                if let Some(entry) = standings.player_mut(target) {
                    entry.score += points;
                }
                Vec::new()
            }
            ("team", [UiValue::Team(team)]) => vec![data(UiScope::Team(*team), "called", "1")],
            ("reset", []) => vec![data(UiScope::All, "joined-in", "")],
            ("echo", args) => vec![data(UiScope::All, "echo", &format!("{args:?}"))],
            ("crash", []) => panic!("the mode crashed"),
            _ => Vec::new(),
        }
    }
}

fn driver() -> Driver {
    Driver::start(&Native::<Votes>::new(), "{}", DriverLimits::default()).unwrap()
}

fn action(account_id: &str, event: &str, args: Vec<Parsed>) -> DriverInput {
    DriverInput::UiAction(DriverUiAction {
        account_id: account_id.into(),
        layer: "votes".into(),
        event: event.into(),
        args,
    })
}

/// The values of the commands, as the modescript gets them.
fn ui_data(commands: &[DriverCommand]) -> Vec<DriverNetUiData> {
    commands
        .iter()
        .filter_map(|command| match command {
            DriverCommand::SetUiData(json) => Some(serde_json::from_str(json).unwrap()),
            _ => None,
        })
        .collect()
}

fn value(scope: &str, team: u32, account: &str, key: &str, value: &str) -> DriverNetUiData {
    DriverNetUiData {
        scope: scope.into(),
        team,
        account: account.into(),
        keys: vec![key.into()],
        values: vec![value.into()],
    }
}

#[test]
fn actions_reach_the_mode_with_their_arguments() {
    let mut driver = driver();

    let joined = driver.apply(&action("a", "join", Vec::new())).unwrap();
    assert_eq!(
        ui_data(&joined.commands),
        [
            value("account", 0, "a", "joined", "1"),
            value("all", 0, "", "joined-in", "votes"),
        ]
    );
    // The standings the mode changed go out like after any other event.
    assert!(
        joined
            .commands
            .iter()
            .any(|command| matches!(command, DriverCommand::SetStandings(_)))
    );

    let args = vec![Parsed::Int(3), Parsed::User("b".into())];
    driver.apply(&action("a", "score", args)).unwrap();
    let scores: Vec<(String, i32)> = driver
        .standings()
        .entries
        .iter()
        .map(|entry| (entry.account_id.clone(), entry.score))
        .collect();
    assert_eq!(scores, [("a".to_string(), 0), ("b".to_string(), 3)]);

    let called = driver
        .apply(&action("c", "team", vec![Parsed::Team(7)]))
        .unwrap();
    assert_eq!(
        ui_data(&called.commands),
        [value("team", 7, "", "called", "1")]
    );

    // Every kind of argument reaches the mode as it was parsed.
    let args = vec![
        Parsed::Int(-2),
        Parsed::Bool(true),
        Parsed::Bool(false),
        Parsed::Text("gg".into()),
        Parsed::Choice(3),
        Parsed::User("d".into()),
        Parsed::Team(9),
    ];
    let echoed = driver.apply(&action("c", "echo", args)).unwrap();
    let expected = [
        UiValue::Int(-2),
        UiValue::Bool(true),
        UiValue::Bool(false),
        UiValue::Text("gg".into()),
        UiValue::Choice(3),
        UiValue::User("d".into()),
        UiValue::Team(9),
    ];
    assert_eq!(
        ui_data(&echoed.commands),
        [value("all", 0, "", "echo", &format!("{expected:?}"))]
    );
}

#[test]
fn a_resumed_modescript_gets_the_values_back() {
    let mut driver = driver();
    driver.apply(&action("a", "join", Vec::new())).unwrap();
    driver
        .apply(&action("c", "team", vec![Parsed::Team(7)]))
        .unwrap();
    // An empty value removes its key, it does not come back.
    let reset = driver.apply(&action("a", "reset", Vec::new())).unwrap();
    assert_eq!(
        ui_data(&reset.commands),
        [value("all", 0, "", "joined-in", "")]
    );

    let resumed = driver.apply(&DriverInput::Resumed).unwrap();
    assert_eq!(
        ui_data(&resumed.commands),
        [
            value("account", 0, "a", "joined", "1"),
            value("team", 7, "", "called", "1"),
        ]
    );

    // A modescript that starts again starts a match, it gets them as well. What the
    // mode sets for the new match comes after them, so it wins.
    let restarted = driver.handle(&match_start()).unwrap();
    let mut expected = ui_data(&resumed.commands);
    expected.push(value("all", 0, "", "phase", "start"));
    assert_eq!(ui_data(&restarted.commands), expected);
    let again = driver.apply(&DriverInput::Resumed).unwrap();
    assert!(ui_data(&again.commands).contains(&value("all", 0, "", "phase", "start")));
}

#[test]
fn a_mode_which_failed_gets_no_actions() {
    let mut driver = driver();
    assert!(matches!(
        driver.apply(&action("a", "crash", Vec::new())),
        Err(DriverError::Fault(_))
    ));
    assert_eq!(
        driver.apply(&action("a", "join", Vec::new())),
        Err(DriverError::Poisoned)
    );
}

#[test]
fn actions_replay_from_their_records() {
    let source = Native::<Votes>::new();
    let mut recording = RecordingDriver::start(&source, "{}", DriverLimits::default()).unwrap();
    let mut records = Vec::new();
    let inputs = [
        action("a", "join", Vec::new()),
        action("a", "score", vec![Parsed::Int(2), Parsed::User("b".into())]),
        action("b", "score", vec![Parsed::Int(5), Parsed::User("b".into())]),
    ];
    for input in &inputs {
        let (mut more, _) = recording.apply(input).unwrap();
        records.append(&mut more);
    }

    // Players go by their number, a's before its first action, b's before its own.
    let actions: Vec<(u32, &str, usize)> = records
        .iter()
        .filter_map(|record| match record {
            DriverRecord::UiAction(action) => {
                Some((action.player, action.event.as_str(), action.args.len()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(actions, [(0, "join", 0), (0, "score", 2), (1, "score", 2)]);

    let replayed =
        RecordedDriver::replay(&source, "{}", DriverLimits::default(), &records).unwrap();
    assert_eq!(replayed.driver().digest(), recording.driver().digest());
    assert_eq!(
        replayed.driver().standings(),
        recording.driver().standings()
    );
    assert_eq!(replayed.account(1), Some("b"));
    let points: i32 = replayed
        .driver()
        .standings()
        .entries
        .iter()
        .map(|e| e.score)
        .sum();
    assert_eq!(points, 7);
}
