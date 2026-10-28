use tm_driver_sdk::{Label, Race, Racer, WarmUp, testing::Harness};

use super::*;

/// Drove the best race with these race times at the checkpoints, the finish last,
/// and then a race a second slower. In time attack rounds the best race counts,
/// not the last one.
fn finish_at(account_id: &str, checkpoints: &[u32]) -> Racer {
    let slower = checkpoints.iter().map(|time| time + 1_000).collect();
    Racer {
        account_id: account_id.into(),
        races: vec![
            Race {
                checkpoints: checkpoints.to_vec(),
                finished: true,
            },
            Race {
                checkpoints: slower,
                finished: true,
            },
        ],
    }
}

fn finish(account_id: &str, best_time: u32) -> Racer {
    finish_at(account_id, &[best_time])
}

/// Gave up after the first checkpoint and never finished.
fn dnf(account_id: &str) -> Racer {
    Racer {
        account_id: account_id.into(),
        races: vec![Race {
            checkpoints: vec![10_000],
            finished: false,
        }],
    }
}

fn round_end(racers: Vec<Racer>) -> Event {
    Event::RoundEnd(RoundEnd {
        invalid: false,
        racers,
        present: Vec::new(),
    })
}

fn started(settings: &str) -> Harness<TimeAttack> {
    let mut harness = Harness::new(settings).unwrap();
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    harness
}

/// Account, position and points of the map of every player in the order of the
/// standings. Every map is a single round.
fn map_points(harness: &Harness<TimeAttack>) -> Vec<(&str, u32, i32)> {
    harness
        .standings
        .entries
        .iter()
        .map(|p| {
            let round = harness.round_entry(&p.account_id).map_or(0, |r| r.score);
            (p.account_id.as_str(), p.position, round)
        })
        .collect()
}

#[test]
fn sets_the_round_format_and_warmup_on_map_start() {
    let mut harness = Harness::<TimeAttack>::new(
        r#"{"time_limit_ms": 60000, "warmup": {"rounds": 1, "timeout_ms": 5000}}"#,
    )
    .unwrap();
    assert_eq!(
        harness.step(Event::MapStart),
        [
            Command::SetRoundFormat(RoundFormat::TimeAttack(60_000)),
            Command::SetWarmUp(WarmUp {
                rounds: 1,
                duration_ms: 0,
                timeout_ms: Some(5000),
            }),
        ]
    );
    assert_eq!(harness.standings.info, "Map 1 / 1");
}

#[test]
fn default_points_keep_the_order_of_times() {
    let mut harness = started("");
    let commands = harness.step(round_end(vec![
        finish("b", 41_000),
        finish("a", 40_000),
        dnf("c"),
    ]));

    assert_eq!(
        map_points(&harness),
        [("a", 1, 2), ("b", 2, 1), ("c", 3, 0)]
    );
    assert_eq!(commands, [Command::EndMap]);
}

#[test]
fn points_repartition() {
    let mut harness = started(r#"{"points_repartition": [10, 5]}"#);
    harness.step(round_end(vec![
        finish("a", 40_000),
        finish("b", 41_000),
        finish("c", 42_000),
    ]));
    assert_eq!(
        map_points(&harness),
        [("a", 1, 10), ("b", 2, 5), ("c", 3, 5)]
    );
}

#[test]
fn equal_best_times_go_to_the_better_checkpoints() {
    let mut harness = started("");
    // b passed the last checkpoint of the best race earlier.
    harness.step(round_end(vec![
        finish_at("a", &[20_500, 40_000]),
        finish_at("b", &[20_400, 40_000]),
    ]));
    assert_eq!(map_points(&harness), [("b", 1, 2), ("a", 2, 1)]);
}

#[test]
fn invalid_rounds_replay_the_map() {
    let mut harness = started("");
    let commands = harness.step(Event::RoundEnd(RoundEnd {
        invalid: true,
        racers: vec![finish("a", 40_000)],
        present: Vec::new(),
    }));
    assert!(commands.is_empty());
    assert!(harness.standings.entries.is_empty());
}

#[test]
fn points_add_up_over_maps() {
    let mut harness = started(r#"{"maps_per_match": 2}"#);
    harness.step(round_end(vec![finish("a", 40_000), finish("b", 41_000)]));
    assert!(harness.step(Event::MapEnd).is_empty());

    harness.step(Event::MapStart);
    assert_eq!(harness.standings.info, "Map 2 / 2");
    harness.step(round_end(vec![finish("b", 39_000), finish("a", 40_000)]));
    assert_eq!(harness.standing("a").score, 3);
    assert_eq!(harness.standing("b").score, 3);
    assert_eq!(
        harness.round_entry("b").map(|r| (r.position, r.score)),
        Some((1, 2))
    );

    // Tied on points, b won the last map.
    assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
    assert_eq!(harness.standing("b").position, 1);
    assert_eq!(harness.standing("b").label, Some(Label::winner()));
    assert_eq!(harness.standing("a").position, 2);
    assert_eq!(harness.standing("a").label, None);
}

#[test]
fn plays_the_playlist_once() {
    let mut harness = Harness::<TimeAttack>::new("").unwrap();
    harness.map_count = 3;
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    assert_eq!(harness.standings.info, "Map 1 / 3");
    for _ in 0..2 {
        harness.step(round_end(vec![finish("a", 40_000)]));
        assert!(harness.step(Event::MapEnd).is_empty());
        harness.step(Event::MapStart);
    }
    harness.step(round_end(vec![finish("a", 40_000)]));
    assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
}

#[test]
fn rejects_invalid_settings() {
    assert!(TimeAttack::from_json(r#"{"time_limit": 300}"#).is_err());
    assert!(TimeAttack::from_json(r#"{"time_limit_ms": 0}"#).is_err());
}
