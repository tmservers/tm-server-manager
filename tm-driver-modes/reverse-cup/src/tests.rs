use tm_driver_sdk::{FinishTimeout, Race, WarmUp, testing::Harness};

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

/// Finished with these race times at the checkpoints, the finish last.
fn finish_at(account_id: &str, checkpoints: &[u32]) -> Racer {
    racer(account_id, checkpoints, true)
}

fn finish(account_id: &str, time: u32) -> Racer {
    finish_at(account_id, &[time / 2, time / 2, time])
}

/// Gave up after passing a number of checkpoints.
fn dnf(account_id: &str, progress: u32) -> Racer {
    let checkpoints: Vec<u32> = (1..=progress)
        .map(|checkpoint| checkpoint * 10_000)
        .collect();
    racer(account_id, &checkpoints, false)
}

fn round_end(racers: Vec<Racer>) -> Event {
    watched_round_end(racers, &[])
}

/// A round in which the players `watching` were on the server without racing.
fn watched_round_end(racers: Vec<Racer>, watching: &[&str]) -> Event {
    Event::RoundEnd(RoundEnd {
        invalid: false,
        racers,
        present: watching.iter().map(|&p| p.into()).collect(),
    })
}

fn standing(account_id: &str, status: Status, points: i32, position: u32) -> Entry {
    Entry {
        account_id: account_id.into(),
        position,
        score: points,
        time: None,
        status,
        label: None,
    }
}

fn alive(account_id: &str, points: i32) -> Entry {
    standing(account_id, Status::Playing, points, 0)
}

fn last_chance(account_id: &str) -> Entry {
    standing(account_id, Status::Playing, 0, 0)
}

/// Players in last chance still play without points.
const IN_LAST_CHANCE: (Status, i32) = (Status::Playing, 0);

fn eliminated(account_id: &str, rank: u32) -> Entry {
    standing(account_id, Status::Eliminated, 0, rank)
}

fn started(settings: &str) -> Harness<ReverseCup> {
    let mut harness = Harness::new(settings).unwrap();
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    harness
}

/// A match whose participants already reached these standings. They lost points,
/// so registration is closed.
fn playing(settings: &str, players: Vec<Entry>) -> Harness<ReverseCup> {
    let mut harness = started(settings);
    harness.standings.entries = players;
    harness
}

fn race_start(account_id: &str) -> Event {
    Event::RaceStart(account_id.into())
}

fn status(harness: &Harness<ReverseCup>, account_id: &str) -> (Status, i32) {
    let player = harness.standing(account_id);
    (player.status, player.score)
}

fn round_points<'a>(harness: &'a Harness<ReverseCup>, accounts: &[&'a str]) -> Vec<(&'a str, i32)> {
    accounts
        .iter()
        .map(|&account_id| {
            let round = harness.round_entry(account_id).map_or(0, |r| r.score);
            (account_id, round)
        })
        .collect()
}

fn chat(commands: &[Command]) -> Vec<&str> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::ChatMessage(message) => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

/// Players who cannot spawn anymore.
fn blocked(commands: &[Command]) -> Vec<&str> {
    commands
        .iter()
        .filter_map(|c| match c {
            Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn: false,
            }) => Some(account_id.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn sets_the_round_format_and_warmup_on_map_start() {
    let mut harness = Harness::<ReverseCup>::new(
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
fn players_who_start_the_first_round_take_part() {
    let mut harness = started("");
    let commands = harness.step(watched_round_end(
        vec![finish("a", 30_000), dnf("b", 2)],
        &["s"],
    ));
    let participants: Vec<&str> = harness
        .standings
        .participants()
        .map(|p| p.account_id.as_str())
        .collect();
    assert_eq!(participants, ["a", "b"]);
    assert_eq!(blocked(&commands), ["s"]);
    assert_eq!(harness.standing("s").status, Status::Spectator);
    // The first round counts already. Two players lose 4 and 8 points.
    assert_eq!(round_points(&harness, &["a", "b"]), [("a", -4), ("b", -20)]);
    assert_eq!(status(&harness, "a"), (Status::Playing, 96));
    assert_eq!(status(&harness, "b"), (Status::Playing, 80));

    // Late players cannot spawn anymore once they show up and their races do
    // not count. The host leaves out s, who got blocked before.
    let commands = harness.step(watched_round_end(
        vec![
            finish("c", 29_000),
            finish("a", 30_000),
            finish("b", 31_000),
        ],
        &["s"],
    ));
    assert_eq!(blocked(&commands), ["c", "s"]);
    assert_eq!(round_points(&harness, &["a", "b"]), [("a", -4), ("b", -8)]);
    assert_eq!(harness.standing("c").status, Status::Spectator);
    assert_eq!(harness.standing("c").position, 0);
    assert_eq!(harness.standings.info, "2 left, 2 / 5 rounds");
}

#[test]
fn players_take_part_from_their_first_race() {
    let mut harness = started("");
    assert!(harness.step(race_start("a")).is_empty());
    harness.step(race_start("b"));
    assert_eq!(status(&harness, "a"), (Status::Playing, 100));
    // Without a position until the first round.
    assert_eq!(harness.standing("a").position, 0);
    assert_eq!(harness.standings.info, "2 left, 0 / 5 rounds");

    harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
    assert_eq!(status(&harness, "b"), (Status::Playing, 92));

    // The first round counted, so later players do not take part anymore.
    harness.step(race_start("c"));
    assert!(harness.standings.player("c").is_none());
    let commands = harness.step(round_end(vec![
        finish("c", 29_000),
        finish("a", 30_000),
        finish("b", 31_000),
    ]));
    assert_eq!(blocked(&commands), ["c"]);
    assert_eq!(harness.standing("c").status, Status::Spectator);
}

#[test]
fn waits_for_more_players_than_winners() {
    let mut harness = started(r#"{"number_of_winners": 2}"#);
    harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
    // Two winners would leave nobody to lose, so the round does not count.
    assert_eq!(status(&harness, "a"), (Status::Playing, 100));
    assert!(harness.round.is_empty());
    assert_eq!(harness.standings.info, "Waiting for players, 2 / 3");

    // The match waits on the next map too, e.g. after an admin skipped this one.
    assert!(harness.step(Event::MapEnd).is_empty());
    harness.step(Event::MapStart);
    assert_eq!(harness.standings.info, "Waiting for players, 2 / 3");

    // A third player starts the match. Three players lose 2, 6 and 9 points.
    harness.step(round_end(vec![
        finish("a", 30_000),
        finish("b", 31_000),
        finish("c", 32_000),
    ]));
    assert_eq!(status(&harness, "c"), (Status::Playing, 91));
    assert_eq!(harness.standings.info, "3 left, 1 / 5 rounds");
}

#[test]
fn invalid_rounds_do_not_count() {
    let invalid = |racers, watching| {
        let Event::RoundEnd(round_end) = watched_round_end(racers, watching) else {
            unreachable!()
        };
        Event::RoundEnd(RoundEnd {
            invalid: true,
            ..round_end
        })
    };
    // Players who started take part, but nobody loses points and nobody is blocked
    // while registration is open.
    let mut harness = started("");
    assert!(
        harness
            .step(invalid(vec![finish("a", 30_000)], &["s"]))
            .is_empty()
    );
    assert_eq!(status(&harness, "a"), (Status::Playing, 100));
    assert!(harness.standings.player("s").is_none());

    let mut harness = playing("", vec![alive("a", 50)]);
    let commands = harness.step(invalid(vec![finish("a", 30_000), finish("c", 31_000)], &[]));
    assert_eq!(blocked(&commands), ["c"]);
    assert_eq!(status(&harness, "a"), (Status::Playing, 50));
    assert_eq!(harness.standings.rounds_on_map, 0);
}

#[test]
fn points_lost_by_finish_order() {
    let mut harness = playing(
        r#"{"points_repartition": [1, 2], "complex_points_repartition": {}}"#,
        vec![
            alive("a", 50),
            alive("b", 50),
            alive("c", 50),
            alive("d", 50),
        ],
    );
    harness.step(round_end(vec![
        finish("b", 31_000),
        finish("a", 30_000),
        dnf("d", 1),
        finish("c", 32_000),
    ]));
    assert_eq!(
        round_points(&harness, &["a", "b", "c", "d"]),
        [
            ("a", -1),
            ("b", -2),
            // Beyond the repartition everyone loses the last value.
            ("c", -2),
            ("d", -20),
        ]
    );
    assert_eq!(status(&harness, "d"), (Status::Playing, 30));
}

#[test]
fn equal_times_go_to_the_better_checkpoints() {
    let mut harness = playing(
        r#"{"points_repartition": [1, 2], "complex_points_repartition": {}}"#,
        vec![alive("a", 50), alive("b", 50)],
    );
    // b passed the last checkpoint earlier and loses less.
    harness.step(round_end(vec![
        finish_at("a", &[10_000, 20_500, 30_000]),
        finish_at("b", &[10_100, 20_400, 30_000]),
    ]));
    assert_eq!(round_points(&harness, &["a", "b"]), [("a", -2), ("b", -1)]);
    assert_eq!(harness.standing("b").position, 1);
}

#[test]
fn repartition_by_players_left() {
    let mode =
        ReverseCup::from_json(r#"{"complex_points_repartition": {"2, 3": [5, 10]}}"#).unwrap();
    let mut standings = Standings {
        entries: vec![alive("a", 50), alive("b", 50), alive("c", 50)],
        ..Default::default()
    };
    assert_eq!(mode.repartition(&standings), [5, 10]);

    let mut mode = ReverseCup::from_json(
        r#"{"points_repartition": [1, 2, 3], "complex_points_repartition": {}}"#,
    )
    .unwrap();
    standings.entries = vec![
        alive("a", 50),
        alive("b", 50),
        alive("c", 50),
        alive("d", 50),
        eliminated("e", 5),
    ];
    // More players left than values: the eliminated player removes the first value.
    assert_eq!(mode.repartition(&standings), [2, 3]);
    mode.settings.fast_forward_points_repartition = false;
    assert_eq!(mode.repartition(&standings), [1, 2, 3]);

    mode.settings.fast_forward_points_repartition = true;
    standings.entries.remove(0);
    assert_eq!(mode.repartition(&standings), [1, 2, 3]);
}

#[test]
fn last_chance_ends_by_losing_the_most() {
    let mut harness = playing(
        r#"{"points_repartition": [5, 10], "complex_points_repartition": {}}"#,
        vec![alive("a", 20), alive("b", 8), last_chance("c")],
    );
    let commands = harness.step(round_end(vec![
        finish("a", 30_000),
        finish("b", 31_000),
        finish("c", 32_000),
    ]));
    assert_eq!(status(&harness, "a"), (Status::Playing, 15));
    // Players with points get a last chance, even if they lost the most.
    assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
    assert_eq!(harness.standing("b").label, Some(last_chance_label()));
    assert_eq!(status(&harness, "c"), (Status::Eliminated, 0));
    assert_eq!(harness.standing("c").label, None);
    assert_eq!(harness.standing("c").position, 3);
    assert_eq!(blocked(&commands), ["c"]);
    assert_eq!(chat(&commands), ["$f90$i$<c$> takes 3rd place!"]);
    assert!(!commands.contains(&Command::EndMatch));
}

#[test]
fn last_chance_survives_when_someone_else_lost_more() {
    let mut harness = playing("", vec![alive("a", 50), last_chance("b"), last_chance("c")]);
    let commands = harness.step(round_end(vec![
        finish("b", 30_000),
        finish("c", 31_000),
        dnf("a", 2),
    ]));
    assert_eq!(
        round_points(&harness, &["a", "b", "c"]),
        [("a", -20), ("b", -2), ("c", -6)]
    );
    assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
    assert_eq!(status(&harness, "c"), IN_LAST_CHANCE);
    assert!(chat(&commands).is_empty());
}

#[test]
fn only_the_least_checkpoints_lose_in_last_chance() {
    let mut harness = playing(
        r#"{"last_chance_dnf_mode": "only_least_checkpoints"}"#,
        vec![
            last_chance("a"),
            last_chance("b"),
            last_chance("c"),
            last_chance("d"),
            alive("e", 50),
        ],
    );
    // d did not start.
    harness.step(round_end(vec![
        finish("a", 30_000),
        dnf("b", 3),
        dnf("c", 1),
        dnf("e", 2),
    ]));
    assert_eq!(
        round_points(&harness, &["a", "b", "c", "d", "e"]),
        [
            // Five players lose 0, 4, 7, 10 and 13 points.
            ("a", 0),
            ("b", 0),
            ("c", -20),
            // Did not start, for example because they left.
            ("d", -20),
            ("e", -20),
        ]
    );
    assert_eq!(status(&harness, "a"), IN_LAST_CHANCE);
    assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
    assert_eq!(status(&harness, "c"), (Status::Eliminated, 0));
    assert_eq!(status(&harness, "d"), (Status::Eliminated, 0));
    // c raced, d did not.
    assert_eq!(harness.standing("c").position, 4);
    assert_eq!(harness.standing("d").position, 5);
    assert_eq!(status(&harness, "e"), (Status::Playing, 30));
}

#[test]
fn every_dnf_loses_in_last_chance_by_default() {
    let mut harness = playing(
        "",
        vec![last_chance("a"), last_chance("b"), last_chance("c")],
    );
    harness.step(round_end(vec![
        finish("a", 30_000),
        dnf("b", 3),
        dnf("c", 1),
    ]));
    assert_eq!(harness.standing("b").status, Status::Eliminated);
    // b got further than c.
    assert_eq!(harness.standing("b").position, 2);
    assert_eq!(harness.standing("c").position, 3);
}

#[test]
fn eliminates_without_last_chance() {
    let mut harness = playing(
        r#"{"disable_last_chance": true, "points_repartition": [5, 10], "complex_points_repartition": {}}"#,
        vec![alive("a", 20), alive("b", 8), alive("c", 30)],
    );
    harness.step(round_end(vec![
        finish("a", 30_000),
        finish("b", 31_000),
        finish("c", 32_000),
    ]));
    assert_eq!(status(&harness, "a"), (Status::Playing, 15));
    assert_eq!(status(&harness, "b"), (Status::Eliminated, 0));
    assert_eq!(harness.standing("b").position, 3);
    assert_eq!(status(&harness, "c"), (Status::Playing, 20));
}

#[test]
fn match_ends_once_the_winners_are_left() {
    let mut harness = playing("", vec![alive("a", 50), last_chance("b"), last_chance("c")]);
    let commands = harness.step(round_end(vec![
        finish("a", 30_000),
        dnf("b", 2),
        dnf("c", 2),
    ]));
    // Equal races go by account.
    assert_eq!(harness.standing("b").position, 2);
    assert_eq!(harness.standing("c").position, 3);
    assert_eq!(status(&harness, "a"), (Status::Playing, 48));
    assert_eq!(harness.standing("a").label, Some(Label::winner()));
    assert_eq!(harness.standing("a").position, 1);
    assert_eq!(
        chat(&commands),
        [
            "$f90$i$<b$> takes 2nd place!",
            "$f90$i$<c$> takes 3rd place!",
            "$<a$> wins the match!",
        ]
    );
    assert_eq!(commands.last(), Some(&Command::EndMatch));
    assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
}

#[test]
fn nobody_wins_while_players_can_join() {
    let mut harness = started(r#"{"number_of_winners": 2}"#);
    harness.step(race_start("a"));
    harness.step(race_start("b"));
    assert_eq!(harness.standing("a").label, None);
    assert_eq!(harness.standing("b").label, None);
}

#[test]
fn match_of_one_player_ends_with_the_elimination() {
    // One player loses 1 point as the first place.
    let mut harness = playing("", vec![alive("a", 3)]);
    let commands = harness.step(round_end(vec![finish("a", 30_000)]));
    assert_eq!(status(&harness, "a"), (Status::Playing, 2));
    assert!(!commands.contains(&Command::EndMatch));

    harness.step(round_end(vec![finish("a", 30_000)]));
    assert_eq!(status(&harness, "a"), IN_LAST_CHANCE);
    let commands = harness.step(round_end(vec![finish("a", 30_000)]));
    assert_eq!(harness.standing("a").status, Status::Eliminated);
    assert_eq!(harness.standing("a").position, 1);
    assert_eq!(chat(&commands), ["$f90$i$<a$> takes 1st place!"]);
    assert_eq!(commands.last(), Some(&Command::EndMatch));
}

#[test]
fn fast_forwards_when_nobody_can_keep_points() {
    let players = vec![alive("a", 6), alive("b", 12), eliminated("c", 3)];
    let mut harness = playing("", players.clone());
    let commands = harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
    // a has 2 and b 4 points left. The next round costs the first place 4 points.
    assert_eq!(status(&harness, "a"), IN_LAST_CHANCE);
    assert_eq!(status(&harness, "b"), IN_LAST_CHANCE);
    assert_eq!(harness.standing("b").label, Some(last_chance_label()));
    assert_eq!(
        chat(&commands),
        ["$c00Fast forward: every remaining player is now in Last Chance."]
    );

    let mut harness = playing(r#"{"allow_fast_forward_rounds": false}"#, players);
    harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
    assert_eq!(status(&harness, "a"), (Status::Playing, 2));
    assert_eq!(status(&harness, "b"), (Status::Playing, 4));
}

#[test]
fn map_ends_after_rounds_with_lost_points() {
    let mut harness = playing(
        r#"{"rounds_per_map": 1, "complex_points_repartition": {"2": [0, 0]}}"#,
        vec![alive("a", 50), alive("b", 50)],
    );
    // Nobody lost points, so the round does not count.
    let commands = harness.step(round_end(vec![finish("a", 30_000), finish("b", 31_000)]));
    assert!(!commands.contains(&Command::EndMap));

    let commands = harness.step(round_end(vec![finish("a", 30_000), dnf("b", 0)]));
    assert_eq!(commands.last(), Some(&Command::EndMap));
    // Next map, the match goes on.
    assert!(harness.step(Event::MapEnd).is_empty());
}

#[test]
fn ranks_by_points_then_last_chance_then_elimination() {
    let mut harness = playing(
        r#"{"rounds_per_map": 5}"#,
        vec![
            alive("a", 50),
            last_chance("b"),
            eliminated("c", 5),
            alive("d", 60),
            alive("e", 5),
        ],
    );
    // Four players lose 0, 4, 7 and 10 points.
    harness.step(round_end(vec![
        finish("d", 30_000),
        finish("a", 31_000),
        finish("e", 32_000),
        finish("b", 33_000),
    ]));

    let standings: Vec<(&str, u32, Status, i32, i32)> = harness
        .standings
        .entries
        .iter()
        .map(|p| {
            let round = harness.round_entry(&p.account_id).map_or(0, |r| r.score);
            (p.account_id.as_str(), p.position, p.status, p.score, round)
        })
        .collect();
    assert_eq!(
        standings,
        [
            ("d", 1, Status::Playing, 60, 0),
            ("a", 2, Status::Playing, 46, -4),
            // In last chance.
            ("e", 3, Status::Playing, 0, -7),
            ("b", 4, Status::Eliminated, 0, -10),
            ("c", 5, Status::Eliminated, 0, 0),
        ]
    );
    assert_eq!(harness.standings.info, "3 left, 1 / 5 rounds");

    // Places follow the race, c was eliminated before and did not race.
    let places: Vec<(&str, u32, Status)> = harness
        .round
        .iter()
        .map(|p| (p.account_id.as_str(), p.position, p.status))
        .collect();
    assert_eq!(
        places,
        [
            ("d", 1, Status::Playing),
            ("a", 2, Status::Playing),
            ("e", 3, Status::Playing),
            ("b", 4, Status::Eliminated)
        ]
    );
}

#[test]
fn rejects_invalid_settings() {
    assert!(ReverseCup::from_json(r#"{"starting_point": 100}"#).is_err());
    assert!(ReverseCup::from_json(r#"{"complex_points_repartition": {"two": [4, 8]}}"#).is_err());
    assert!(ReverseCup::from_json(r#"{"last_chance_dnf_mode": "only_least_checkpoints"}"#).is_ok());
}

#[test]
fn starting_points_cannot_change_during_a_match() {
    tm_driver_sdk::testing::assert_reloadable_settings_exist::<ReverseCup>();
    let mut harness = started(r#"{"starting_points": 50}"#);
    assert_eq!(
        harness.change_settings(r#"{"starting_points": 60}"#),
        Err("starting_points cannot change during a match".into())
    );
    assert!(
        harness
            .change_settings(r#"{"starting_points": 50, "points_repartition": [5, 10]}"#)
            .is_ok()
    );
}

#[test]
fn more_winners_end_the_match_with_the_next_round() {
    let mut harness = playing("", vec![alive("a", 50), alive("b", 40), alive("c", 30)]);
    harness
        .change_settings(r#"{"number_of_winners": 3}"#)
        .unwrap();
    let commands = harness.step(round_end(vec![
        finish("a", 30_000),
        finish("b", 31_000),
        finish("c", 32_000),
    ]));
    assert_eq!(commands.last(), Some(&Command::EndMatch));
    assert_eq!(chat(&commands), ["$<a$>, $<b$>, $<c$> win the match!"]);
    // The round that ended it took no points.
    assert_eq!(status(&harness, "c"), (Status::Playing, 30));
}

#[test]
fn the_live_round_shows_what_the_finishers_so_far_would_lose() {
    let mut harness = playing(
        r#"{"points_repartition": [2, 5, 9], "complex_points_repartition": {}}"#,
        vec![
            alive("a", 50),
            alive("b", 50),
            alive("c", 50),
            alive("d", 50),
        ],
    );
    let races = RoundEnd {
        invalid: false,
        racers: vec![finish("b", 31_000), dnf("a", 1), finish("c", 30_000)],
        present: Vec::new(),
    };
    let live: Vec<(String, u32, i32)> = harness
        .live_round(races)
        .into_iter()
        .map(|p| (p.account_id, p.position, p.score))
        .collect();
    // a still races and d did not start, whether they lose the dnf points is open.
    assert_eq!(
        live,
        [("c".into(), 1, -2), ("b".into(), 2, -5), ("a".into(), 3, 0)]
    );
    assert_eq!(status(&harness, "b"), (Status::Playing, 50));

    // Nothing counts while the match waits for players.
    harness.standings.entries = vec![alive("a", 100)];
    let waiting = RoundEnd {
        invalid: false,
        racers: vec![finish("a", 30_000)],
        present: Vec::new(),
    };
    assert_eq!(harness.live_round(waiting.clone()).len(), 1);
    let mut harness = started(r#"{"number_of_winners": 2}"#);
    harness.step(race_start("a"));
    harness.step(race_start("b"));
    assert!(harness.live_round(waiting).is_empty());
}
