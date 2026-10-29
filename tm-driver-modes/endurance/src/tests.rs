use tm_driver_sdk::{FinishTimeout, Race, WarmUp, testing::Harness};

use super::*;

/// A race with these race times at the checkpoints and these laps completed.
fn race(checkpoints: &[u32], laps: u32, finished: bool) -> Race {
    Race {
        checkpoints: checkpoints.to_vec(),
        finished,
        laps,
    }
}

fn racer(account_id: &str, races: Vec<Race>) -> Racer {
    Racer {
        account_id: account_id.into(),
        races,
    }
}

/// A race still running with a checkpoint every ten seconds, three of them a lap.
fn driving(account_id: &str, checkpoints: u32) -> Racer {
    let times: Vec<u32> = (1..=checkpoints).map(|n| n * 10_000).collect();
    racer(account_id, vec![race(&times, checkpoints / 3, false)])
}

fn races(racers: Vec<Racer>) -> RoundEnd {
    RoundEnd {
        invalid: false,
        racers,
        present: Vec::new(),
    }
}

fn started(settings: &str, players: &[&str]) -> Harness<Endurance> {
    let mut harness = Harness::new(settings).unwrap();
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    harness.step(Event::RoundStart);
    for player in players {
        harness.step(Event::RaceStart((*player).into()));
    }
    harness
}

/// Account, place, checkpoints, time and label of everyone in a round.
fn places(round: &[Entry]) -> Vec<(&str, u32, i32, Option<u32>, String)> {
    round
        .iter()
        .map(|entry| {
            let label = entry
                .label
                .as_ref()
                .map_or(String::new(), |l| l.text.clone());
            (
                entry.account_id.as_str(),
                entry.position,
                entry.score,
                entry.time,
                label,
            )
        })
        .collect()
}

#[test]
fn infinite_laps_race_until_the_time_limit() {
    let mut harness =
        Harness::<Endurance>::new(r#"{"time_limit_ms": 5400000, "warmup": {"rounds": 1}}"#)
            .unwrap();
    assert_eq!(
        harness.step(Event::MapStart),
        [
            Command::SetRoundFormat(RoundFormat::TimeAttack(5_400_000)),
            Command::SetRaceRules(RaceRules {
                laps: Laps::Infinite,
                give_up: false,
            }),
            Command::SetWarmUp(WarmUp {
                rounds: 1,
                duration_ms: 0,
                timeout_ms: None,
            }),
        ]
    );
    assert_eq!(harness.standings.info, "1:30:00");
}

#[test]
fn laps_race_until_everyone_finished() {
    let mut harness =
        Harness::<Endurance>::new(r#"{"laps": 50, "finish_timeout": {"milliseconds": 60000}}"#)
            .unwrap();
    let commands = harness.step(Event::MapStart);
    assert_eq!(
        commands[..2],
        [
            Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                60_000
            ))),
            Command::SetRaceRules(RaceRules {
                laps: Laps::Count(50),
                give_up: false,
            }),
        ]
    );
    assert_eq!(harness.standings.info, "50 laps");
}

#[test]
fn the_most_checkpoints_lead_live() {
    let harness = started("{}", &["a", "b", "c", "d"]);
    let live = harness.live_round(races(vec![
        driving("a", 7),
        driving("b", 8),
        // As far as a, but got there a second earlier.
        racer(
            "c",
            vec![race(
                &[10_000, 20_000, 30_000, 40_000, 50_000, 60_000, 69_000],
                2,
                false,
            )],
        ),
        // Just started.
        racer("d", vec![race(&[], 0, false)]),
    ]));
    assert_eq!(
        places(&live),
        [
            ("b", 1, 8, Some(80_000), "Lap 3".into()),
            ("c", 2, 7, Some(69_000), "Lap 3".into()),
            ("a", 3, 7, Some(70_000), "Lap 3".into()),
            ("d", 4, 0, None, "Lap 1".into()),
        ]
    );
    // The match only has a result once the race ended.
    assert!(harness.standings.entries.iter().all(|p| p.position == 0));
}

#[test]
fn checkpoints_of_every_race_count() {
    let harness = started("{}", &["a", "b"]);
    // a left after 4 checkpoints and came back for 3 more, b drove 6 in one go.
    let live = harness.live_round(races(vec![
        racer(
            "a",
            vec![
                race(&[10_000, 20_000, 30_000, 40_000], 1, false),
                race(&[10_000, 20_000, 30_000], 1, false),
            ],
        ),
        driving("b", 6),
    ]));
    assert_eq!(
        places(&live),
        [
            ("a", 1, 7, Some(70_000), "Lap 3".into()),
            ("b", 2, 6, Some(60_000), "Lap 3".into()),
        ]
    );
}

#[test]
fn the_race_is_the_result_of_the_match() {
    let mut harness = started(r#"{"laps": 2}"#, &["a", "b", "c"]);
    let finish = race(&[10_000, 20_000, 30_000, 40_000, 50_000, 60_000], 2, true);
    let commands = harness.step(Event::RoundEnd(races(vec![
        driving("a", 4),
        racer("b", vec![finish]),
        driving("c", 5),
    ])));
    assert_eq!(commands, [Command::EndMatch]);
    assert_eq!(
        places(&harness.round),
        [
            ("b", 1, 6, Some(60_000), "Finished".into()),
            ("c", 2, 5, Some(50_000), "Lap 2 / 2".into()),
            ("a", 3, 4, Some(40_000), "Lap 2 / 2".into()),
        ]
    );
    let standings: Vec<(&str, u32, i32, Option<Label>)> = harness
        .standings
        .entries
        .iter()
        .map(|p| (p.account_id.as_str(), p.position, p.score, p.label.clone()))
        .collect();
    assert_eq!(
        standings,
        [
            ("b", 1, 6, Some(Label::winner())),
            ("c", 2, 5, None),
            ("a", 3, 4, None),
        ]
    );
}

#[test]
fn a_race_that_does_not_count_is_driven_again() {
    let mut harness = started("{}", &["a"]);
    let commands = harness.step(Event::RoundEnd(RoundEnd {
        invalid: true,
        ..races(vec![driving("a", 9)])
    }));
    assert!(commands.is_empty());
    assert_eq!(harness.standing("a").position, 0);
}

#[test]
fn rejects_invalid_settings() {
    tm_driver_sdk::testing::assert_reloadable_settings_exist::<Endurance>();
    assert!(Endurance::from_json(r#"{"laps": 0}"#).is_err());
    assert!(Endurance::from_json(r#"{"time_limit_ms": 0}"#).is_err());
    assert!(Endurance::from_json(r#"{"laps": 3, "time_limit_ms": 0}"#).is_ok());
}
