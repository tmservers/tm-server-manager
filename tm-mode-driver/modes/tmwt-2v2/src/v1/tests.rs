use tm_mode_driver_sdk::{Race, Racer, Team, testing::Harness};

use super::*;

const RED: u32 = 7;
const BLUE: u32 = 3;

fn finish(account_id: &str, time: u32) -> Racer {
    Racer {
        account_id: account_id.into(),
        races: vec![Race {
            checkpoints: vec![time],
            finished: true,
            laps: 1,
        }],
    }
}

fn dnf(account_id: &str) -> Racer {
    Racer {
        account_id: account_id.into(),
        races: vec![Race {
            checkpoints: vec![10_000],
            finished: false,
            laps: 0,
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

/// Red are a and b, Blue are c and d.
fn started(settings: &str) -> Harness<Tmwt2v2> {
    let mut harness = Harness::<Tmwt2v2>::new(settings).unwrap();
    harness.teams = vec![
        Team {
            team_id: RED,
            name: "Red".into(),
            short_name: "R".into(),
            players: vec!["a".into(), "b".into()],
        },
        Team {
            team_id: BLUE,
            name: "Blue".into(),
            short_name: "B".into(),
            players: vec!["c".into(), "d".into()],
        },
    ];
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    harness
}

/// Every team with its position, match points and points on the map, in order.
fn teams(harness: &Harness<Tmwt2v2>) -> Vec<(u32, u32, i32, i32)> {
    harness
        .standings
        .teams
        .iter()
        .map(|t| (t.team_id, t.position, t.score, t.map_score))
        .collect()
}

/// The points of the round of every team.
fn round(harness: &Harness<Tmwt2v2>) -> Vec<(u32, i32)> {
    let mut teams: Vec<(u32, i32)> = harness
        .round_teams
        .iter()
        .map(|t| (t.team_id, t.score))
        .collect();
    teams.sort();
    teams
}

#[test]
fn the_order_of_the_teams_decides_the_round() {
    let cases: [(&[u32], Outcome); 11] = [
        (&[1, 1, 2, 2], Outcome::Ace),
        (&[1, 1, 2], Outcome::Ace),
        (&[1, 1], Outcome::Ace),
        (&[1], Outcome::Ace),
        (&[1, 2, 1, 2], Outcome::Victory),
        (&[1, 2, 1], Outcome::Victory),
        (&[1, 2], Outcome::Victory),
        (&[1, 2, 2, 1], Outcome::Draw),
        (&[1, 2, 2], Outcome::Draw),
        (&[], Outcome::None),
        (&[2, 1, 1, 2], Outcome::Draw),
    ];
    for (order, outcome) in cases {
        assert_eq!(Outcome::of(order), outcome, "{order:?}");
    }
    assert_eq!(Outcome::Ace.points(), (3, 0));
    assert_eq!(Outcome::Victory.points(), (2, 1));
    assert_eq!(Outcome::Draw.points(), (1, 1));
    assert_eq!(Outcome::None.points(), (0, 0));
}

#[test]
fn rounds_give_the_teams_the_points_of_their_order() {
    let mut harness = started("{}");

    // Blue finishes first and third: a victory of Blue.
    harness.step(round_end(vec![
        finish("c", 30_000),
        finish("a", 31_000),
        finish("d", 32_000),
        dnf("b"),
    ]));

    assert_eq!(round(&harness), [(BLUE, 2), (RED, 1)]);
    assert_eq!(teams(&harness), [(BLUE, 1, 0, 2), (RED, 2, 0, 1)]);
    // Finishers get the points of their team.
    assert_eq!(harness.standing("c").score, 2);
    assert_eq!(harness.standing("a").score, 1);
    assert_eq!(harness.standing("b").score, 0);

    // Red finishes first and second: an ace, the third finisher does not matter.
    harness.step(round_end(vec![
        finish("b", 29_000),
        finish("a", 29_500),
        finish("c", 30_000),
    ]));
    assert_eq!(round(&harness), [(BLUE, 0), (RED, 3)]);
    assert_eq!(teams(&harness), [(RED, 1, 0, 4), (BLUE, 2, 0, 2)]);
}

#[test]
fn a_team_wins_the_map_at_the_limit_with_more_points_and_the_match_with_its_maps() {
    let mut harness = started(r#"{"map_points_limit": 3, "match_points_limit": 2}"#);
    let ace_of_red = || round_end(vec![finish("a", 30_000), finish("b", 31_000)]);

    let commands = harness.step(ace_of_red());

    assert_eq!(
        commands,
        [Command::ChatMessage("Ace for Red".into()), Command::EndMap]
    );
    assert_eq!(teams(&harness), [(RED, 1, 1, 3), (BLUE, 2, 0, 0)]);

    harness.step(Event::MapEnd);
    harness.step(Event::MapStart);
    // The points of the map start over, the match points stay.
    assert_eq!(teams(&harness), [(RED, 1, 1, 0), (BLUE, 2, 0, 0)]);

    let commands = harness.step(ace_of_red());

    assert_eq!(
        commands,
        [
            Command::ChatMessage("Ace for Red".into()),
            Command::EndMatch
        ]
    );
    assert_eq!(teams(&harness), [(RED, 1, 2, 3), (BLUE, 2, 0, 0)]);
    assert_eq!(harness.standings.teams[0].label, Some(Label::winner()));
    assert_eq!(harness.standings.info, "Red won the match");
}

#[test]
fn level_teams_at_the_limit_play_overtime() {
    let mut harness = started(r#"{"map_points_limit": 2}"#);
    let draw = || {
        round_end(vec![
            finish("a", 30_000),
            finish("c", 31_000),
            finish("d", 32_000),
            finish("b", 33_000),
        ])
    };

    harness.step(draw());
    let commands = harness.step(draw());

    // 2 to 2: nobody leads, the map goes on.
    assert_eq!(commands, [Command::ChatMessage("Draw".into())]);
    assert_eq!(harness.standings.info, "Map 1, overtime");
    assert!(
        harness
            .standings
            .teams
            .iter()
            .all(|t| t.label == Some(Label::new("Overtime", Tone::Neutral)))
    );

    // A victory of Blue decides it.
    let commands = harness.step(round_end(vec![finish("c", 30_000), finish("a", 31_000)]));
    assert_eq!(
        commands,
        [
            Command::ChatMessage("Victory for Blue".into()),
            Command::EndMap
        ]
    );
    assert_eq!(teams(&harness), [(BLUE, 1, 1, 4), (RED, 2, 0, 3)]);
    assert!(harness.standings.teams.iter().all(|t| t.label.is_none()));
}

#[test]
fn rounds_which_do_not_count_and_rounds_without_finishers_give_no_points() {
    let mut harness = started("{}");

    harness.step(Event::RoundEnd(RoundEnd {
        invalid: true,
        racers: vec![finish("a", 30_000), finish("b", 31_000)],
        present: Vec::new(),
    }));
    harness.step(round_end(vec![dnf("a"), dnf("c")]));

    assert_eq!(teams(&harness), [(BLUE, 1, 0, 0), (RED, 2, 0, 0)]);
    assert_eq!(round(&harness), [(BLUE, 0), (RED, 0)]);
}

#[test]
fn players_never_give_up() {
    let mut harness = Harness::<Tmwt2v2>::new("{}").unwrap();
    harness.step(Event::MatchStart);

    let commands = harness.step(Event::MapStart);

    assert!(commands.contains(&Command::SetRaceRules(RaceRules {
        laps: Laps::Map,
        respawn: Some(Respawn::NeverGiveUp),
    })));
}

#[test]
fn the_live_round_shows_the_points_of_the_teams_so_far() {
    let mut harness = started("{}");
    for player in ["a", "b", "c"] {
        harness.step(Event::RaceStart(player.into()));
    }

    let live = harness.live_round(RoundEnd {
        invalid: false,
        racers: vec![finish("a", 30_000), finish("b", 31_000), dnf("c")],
        present: Vec::new(),
    });

    let points: Vec<(&str, i32)> = live
        .iter()
        .map(|e| (e.account_id.as_str(), e.score))
        .collect();
    assert_eq!(points, [("a", 3), ("b", 3), ("c", 0)]);
}

#[test]
fn settings_need_points_to_win() {
    assert!(Harness::<Tmwt2v2>::new(r#"{"map_points_limit": 0}"#).is_err());
    tm_mode_driver_sdk::testing::assert_reloadable_settings_exist::<Tmwt2v2>();
}

#[test]
fn a_limit_lowered_below_the_maps_of_a_team_ends_the_match_with_the_next_round() {
    let mut harness = started(r#"{"map_points_limit": 3, "match_points_limit": 3}"#);
    let ace_of_red = || round_end(vec![finish("a", 30_000), finish("b", 31_000)]);
    harness.step(ace_of_red());
    harness.step(Event::MapEnd);
    harness.step(Event::MapStart);
    harness.step(ace_of_red());
    harness.step(Event::MapEnd);
    harness.step(Event::MapStart);

    harness
        .change_settings(r#"{"map_points_limit": 3, "match_points_limit": 1}"#)
        .unwrap();
    // Blue wins the round, Red has won two maps.
    let commands = harness.step(round_end(vec![finish("c", 30_000), finish("a", 31_000)]));

    assert_eq!(commands.last(), Some(&Command::EndMatch));
    assert_eq!(harness.standings.teams[0].team_id, RED);
    assert_eq!(harness.standings.teams[0].label, Some(Label::winner()));
}
