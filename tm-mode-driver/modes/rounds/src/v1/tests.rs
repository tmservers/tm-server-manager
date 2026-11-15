use tm_mode_driver_sdk::{FinishTimeout, Label, Race, Racer, WarmUp, testing::Harness};

use super::*;

/// A player who raced once with these race times at the checkpoints.
fn racer(account_id: &str, checkpoints: &[u32], finished: bool) -> Racer {
    Racer {
        account_id: account_id.into(),
        races: vec![Race {
            checkpoints: checkpoints.to_vec(),
            finished,
            laps: u32::from(finished),
        }],
    }
}

fn finish(account_id: &str, time: u32) -> Racer {
    finish_at(account_id, &[time])
}

/// Finished with these race times at the checkpoints, the finish last.
fn finish_at(account_id: &str, checkpoints: &[u32]) -> Racer {
    racer(account_id, checkpoints, true)
}

/// Gave up after the first checkpoint.
fn dnf(account_id: &str) -> Racer {
    racer(account_id, &[10_000], false)
}

fn round_end(racers: Vec<Racer>) -> Event {
    Event::RoundEnd(RoundEnd {
        invalid: false,
        racers,
        present: Vec::new(),
    })
}

fn started(settings: &str) -> Harness<Rounds> {
    let mut harness = Harness::new(settings).unwrap();
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    harness
}

/// Account, position, points and points of the last round of every player in the
/// order of the standings.
fn scores(harness: &Harness<Rounds>) -> Vec<(&str, u32, i32, i32)> {
    harness
        .standings
        .entries
        .iter()
        .map(|p| {
            let round = harness.round_entry(&p.account_id).map_or(0, |r| r.score);
            (p.account_id.as_str(), p.position, p.score, round)
        })
        .collect()
}

#[test]
fn sets_the_round_format_and_warmup_on_map_start() {
    let mut harness = Harness::<Rounds>::new(
        r#"{"finish_timeout": {"milliseconds": 7000}, "warmup": {"rounds": 1, "duration_ms": 60000}}"#,
    )
    .unwrap();
    assert_eq!(
        harness.step(Event::MapStart),
        [
            Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                7000
            ))),
            Command::SetWarmUp(WarmUp {
                rounds: 1,
                duration_ms: 60_000,
                timeout_ms: None,
            }),
        ]
    );
}

#[test]
fn points_by_finish_order() {
    let mut harness = started(r#"{"points_repartition": [10, 6, 4]}"#);
    harness.step(round_end(vec![
        finish("b", 31_000),
        finish("a", 30_000),
        dnf("d"),
        finish("c", 32_000),
        finish("e", 33_000),
    ]));

    assert_eq!(
        scores(&harness),
        [
            ("a", 1, 10, 10),
            ("b", 2, 6, 6),
            // Beyond the repartition everyone gets the last value. The race tells
            // them apart.
            ("c", 3, 4, 4),
            ("e", 4, 4, 4),
            // Raced without a finish.
            ("d", 5, 0, 0),
        ]
    );
    assert_eq!(harness.standings.info, "1 / 5 rounds");

    // Places follow the race, also for the finishers who got the same points.
    let places: Vec<(&str, u32, Option<u32>)> = harness
        .round
        .iter()
        .map(|p| (p.account_id.as_str(), p.position, p.time))
        .collect();
    assert_eq!(
        places,
        [
            ("a", 1, Some(30_000)),
            ("b", 2, Some(31_000)),
            ("c", 3, Some(32_000)),
            ("e", 4, Some(33_000)),
            ("d", 5, None)
        ]
    );
}

#[test]
fn points_add_up_and_round_points_start_over() {
    let mut harness = started(r#"{"points_repartition": [10, 6]}"#);
    harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
    harness.step(round_end(vec![finish("b", 30_000), dnf("a")]));

    assert_eq!(scores(&harness), [("b", 1, 16, 10), ("a", 2, 10, 0)]);
}

#[test]
fn equal_times_go_to_the_better_checkpoints() {
    let mut harness = started(r#"{"points_repartition": [10, 6]}"#);
    // b passed the last checkpoint earlier.
    harness.step(round_end(vec![
        finish_at("a", &[10_000, 20_500, 30_000]),
        finish_at("b", &[10_100, 20_400, 30_000]),
    ]));
    assert_eq!(scores(&harness), [("b", 1, 10, 10), ("a", 2, 6, 6)]);
}

#[test]
fn players_show_up_when_they_start() {
    let mut harness = started("");
    assert!(harness.step(Event::RaceStart("b".into())).is_empty());
    harness.step(Event::RaceStart("a".into()));
    // Without a position until their first round. The host shows them by name.
    assert_eq!(scores(&harness), [("b", 0, 0, 0), ("a", 0, 0, 0)]);
}

#[test]
fn invalid_rounds_do_not_count() {
    let mut harness = started("");
    let commands = harness.step(Event::RoundEnd(RoundEnd {
        invalid: true,
        racers: vec![finish("a", 30_000)],
        present: Vec::new(),
    }));
    assert!(commands.is_empty());
    assert!(harness.standings.entries.is_empty());
    assert!(harness.round.is_empty());
    assert_eq!(harness.standings.rounds_on_map, 0);
}

#[test]
fn map_ends_after_rounds_per_map() {
    let mut harness = started(r#"{"rounds_per_map": 2}"#);
    assert!(
        !harness
            .step(round_end(vec![finish("a", 1), finish("b", 2)]))
            .contains(&Command::EndMap)
    );
    assert!(
        harness
            .step(round_end(vec![finish("a", 1), finish("b", 2)]))
            .contains(&Command::EndMap)
    );

    // One map per match by default. The leader wins.
    assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    assert_eq!(harness.standing("a").label, Some(Label::winner()));
    assert_eq!(harness.standing("b").label, None);
}

#[test]
fn points_add_up_over_maps() {
    let mut harness = started(r#"{"rounds_per_map": 1, "maps_per_match": 2}"#);
    harness.step(round_end(vec![finish("a", 1)]));
    assert!(harness.step(Event::MapEnd).is_empty());

    harness.step(Event::MapStart);
    harness.step(round_end(vec![finish("a", 1)]));
    assert_eq!(harness.standing("a").score, 20);
    assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
}

#[test]
fn plays_the_playlist_once() {
    let mut harness = Harness::<Rounds>::new(r#"{"rounds_per_map": 1}"#).unwrap();
    harness.map_count = 2;
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    assert_eq!(harness.standings.info, "Map 1 / 2, 0 / 1 rounds");
    harness.step(round_end(vec![finish("a", 1)]));
    assert!(harness.step(Event::MapEnd).is_empty());

    harness.step(Event::MapStart);
    harness.step(round_end(vec![finish("a", 1)]));
    assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
}

#[test]
fn points_limit_ends_the_match() {
    let mut harness = started(r#"{"points_repartition": [5, 4], "points_limit": 10}"#);
    let commands = harness.step(round_end(vec![finish("a", 1), finish("b", 2)]));
    assert!(!commands.contains(&Command::EndMatch));

    let commands = harness.step(round_end(vec![finish("a", 1), finish("b", 2)]));
    assert!(commands.contains(&Command::EndMatch));
    assert_eq!(harness.standing("a").label, Some(Label::winner()));
}

#[test]
fn the_last_round_decides_a_tie_at_the_points_limit() {
    let mut harness = started(r#"{"points_repartition": [5, 5], "points_limit": 10}"#);
    harness.step(round_end(vec![finish("a", 1), finish("b", 2)]));
    let commands = harness.step(round_end(vec![finish("b", 1), finish("a", 2)]));
    // Both reached 10 points, b won the last round.
    assert!(commands.contains(&Command::EndMatch));
    assert_eq!(scores(&harness), [("b", 1, 10, 5), ("a", 2, 10, 5)]);
    assert_eq!(harness.standing("b").label, Some(Label::winner()));
    assert_eq!(harness.standing("a").label, None);
}

#[test]
fn rejects_unknown_settings() {
    assert!(Rounds::from_json(r#"{"rounds_per_mpa": 3}"#).is_err());
    // Ties are always broken, so there is nothing to play on for.
    assert!(Rounds::from_json(r#"{"use_tie_break": true}"#).is_err());
}

#[test]
fn every_setting_can_change_during_a_match() {
    tm_mode_driver_sdk::testing::assert_reloadable_settings_exist::<Rounds>();
    assert_eq!(Rounds::RELOADABLE.len(), 6);
}

#[test]
fn new_points_count_from_the_next_round() {
    let mut harness = started(r#"{"points_repartition": [10, 6]}"#);
    harness.step(Event::RoundStart);
    harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));

    // The new settings send the round format again and show in the info.
    let commands = harness
        .change_settings(r#"{"points_repartition": [3, 1], "rounds_per_map": 2}"#)
        .unwrap();
    assert!(matches!(commands.first(), Some(Command::SetRoundFormat(_))));
    assert_eq!(harness.standings.info, "1 / 2 rounds");

    harness.step(Event::RoundStart);
    harness.step(round_end(vec![finish("b", 30_000), finish("a", 31_000)]));
    // The points of the first round stay.
    assert_eq!(scores(&harness), [("a", 1, 11, 1), ("b", 2, 9, 3)]);
}

#[test]
fn rejected_settings_change_nothing() {
    let mut harness = started(r#"{"points_repartition": [10, 6]}"#);
    assert!(harness.change_settings(r#"{"points": [3]}"#).is_err());
    harness.step(Event::RoundStart);
    harness.step(round_end(vec![finish("a", 30_000)]));
    assert_eq!(scores(&harness), [("a", 1, 10, 10)]);
}

/// Account, place, points and time of everyone in a live round.
fn live(entries: &[Entry]) -> Vec<(&str, u32, i32, Option<u32>)> {
    entries
        .iter()
        .map(|p| (p.account_id.as_str(), p.position, p.score, p.time))
        .collect()
}

#[test]
fn the_live_round_gives_the_finishers_so_far_their_points() {
    let mut harness = started(r#"{"points_repartition": [10, 6, 4]}"#);
    harness.step(Event::RoundStart);
    for player in ["a", "b", "c"] {
        harness.step(Event::RaceStart(player.into()));
    }

    // c finished first, a is still racing and b has not passed a checkpoint yet.
    let races = RoundEnd {
        invalid: false,
        racers: vec![
            finish("c", 30_000),
            racer("a", &[10_000], false),
            racer("b", &[], false),
        ],
        present: Vec::new(),
    };
    assert_eq!(
        live(&harness.live_round(races)),
        [
            ("c", 1, 10, Some(30_000)),
            ("a", 2, 0, None),
            ("b", 3, 0, None)
        ]
    );
    // Only the round end awards points.
    assert_eq!(
        scores(&harness),
        [("a", 0, 0, 0), ("b", 0, 0, 0), ("c", 0, 0, 0)]
    );
}
