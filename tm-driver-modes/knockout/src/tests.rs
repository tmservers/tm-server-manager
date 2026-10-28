use tm_driver_sdk::{FinishTimeout, User, WarmUp, testing::Harness};

use super::*;

/// Finished with these race times at the checkpoints, the finish last.
fn finish_at(account_id: &str, checkpoints: &[u32]) -> RaceResult {
    let time = checkpoints.last().copied();
    RaceResult {
        account_id: account_id.into(),
        started: true,
        round_checkpoints: checkpoints.to_vec(),
        previous_time: time,
        previous_checkpoints: checkpoints.to_vec(),
        best_time: time,
        best_checkpoints: checkpoints.to_vec(),
    }
}

fn finish(account_id: &str, time: u32) -> RaceResult {
    finish_at(account_id, &[time / 3, time / 3 * 2, time])
}

/// Gave up after passing checkpoints at these race times.
fn dnf_at(account_id: &str, checkpoints: &[u32]) -> RaceResult {
    RaceResult {
        account_id: account_id.into(),
        started: true,
        round_checkpoints: checkpoints.to_vec(),
        previous_time: None,
        previous_checkpoints: vec![],
        best_time: None,
        best_checkpoints: vec![],
    }
}

/// Gave up after passing a number of checkpoints.
fn dnf(account_id: &str, progress: u32) -> RaceResult {
    let checkpoints: Vec<u32> = (1..=progress)
        .map(|checkpoint| checkpoint * 10_000)
        .collect();
    dnf_at(account_id, &checkpoints)
}

/// Did not start in the round.
fn spectator(account_id: &str) -> RaceResult {
    RaceResult {
        started: false,
        ..dnf(account_id, 0)
    }
}

/// The players finish in this order, one second apart.
fn finishers(players: &[&str]) -> Vec<RaceResult> {
    players
        .iter()
        .enumerate()
        .map(|(index, player)| finish(player, 30_000 + 1_000 * index as u32))
        .collect()
}

fn round_end(results: Vec<RaceResult>) -> Event {
    Event::RoundEnd(RoundEnd {
        invalid: false,
        results,
    })
}

fn race_start(account_id: &str) -> Event {
    Event::RaceStart(account_id.into())
}

fn started(settings: &str) -> Harness<Knockout> {
    let mut harness = Harness::new(settings).unwrap();
    harness.step(Event::MatchStart);
    harness.step(Event::MapStart);
    harness
}

/// A match after its round without elimination, in which these players took
/// part. The next round takes lives.
fn playing(settings: &str, players: &[&str]) -> Harness<Knockout> {
    let mut harness = started(settings);
    harness.step(round_end(finishers(players)));
    harness
}

/// Account, position, status and label of every player in the order of the
/// standings.
fn ranking(harness: &Harness<Knockout>) -> Vec<(&str, u32, Status, &str)> {
    harness
        .standings
        .entries
        .iter()
        .map(|p| {
            let label = p.label.as_ref().map_or("", |label| label.text.as_str());
            (p.account_id.as_str(), p.position, p.status, label)
        })
        .collect()
}

/// Account and lives left of every player in the order of the standings.
fn lives(harness: &Harness<Knockout>) -> Vec<(&str, i32)> {
    harness
        .standings
        .entries
        .iter()
        .map(|p| (p.account_id.as_str(), p.score))
        .collect()
}

/// The hearts of a player with lost lives and lives left.
fn hearts(lost: usize, left: usize) -> String {
    LOST_LIFE.repeat(lost) + &LIFE.repeat(left)
}

/// Gives a player a name like the host does once it knows it.
fn named(harness: &mut Harness<Knockout>, account_id: &str, name: &str) {
    harness.users.insert(
        account_id.into(),
        User {
            account_id: account_id.into(),
            name: name.into(),
            club_tag: String::new(),
            zone: String::new(),
        },
    );
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
    let mut harness = Harness::<Knockout>::new("").unwrap();
    assert_eq!(
        harness.step(Event::MapStart),
        [
            Command::SetRoundFormat(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                5_000
            ))),
            Command::SetWarmUp(WarmUp {
                rounds: 0,
                duration_ms: 0,
                timeout_ms: None,
            }),
        ]
    );
}

#[test]
fn losses_grow_with_the_players_and_lives_left() {
    // The losses of the second round with these players and lives left.
    let losses = |settings: &str, left: &[(usize, usize)]| -> Vec<usize> {
        let mode = Knockout::from_json(settings).unwrap();
        left.iter()
            .map(|&(players, lives)| mode.round_losses(2, players, lives))
            .collect()
    };
    // With one life each.
    let players =
        |left: &[usize]| -> Vec<(usize, usize)> { left.iter().map(|&n| (n, n)).collect() };
    assert_eq!(
        losses("", &players(&[64, 17, 16, 5, 4, 2, 1, 0])),
        [4, 4, 2, 2, 1, 1, 0, 0]
    );
    // 0 adds a loss however many players are left, in any order.
    assert_eq!(
        losses(
            r#"{"extra_losses": {"above_players_left": [8, 0]}}"#,
            &players(&[9, 8, 3, 2])
        ),
        [3, 2, 2, 1]
    );
    assert_eq!(
        losses(
            r#"{"extra_losses": {"above_players_left": []}}"#,
            &players(&[64, 2])
        ),
        [1, 1]
    );
    assert_eq!(
        losses(
            r#"{"lives": 3, "extra_losses": {"above_lives_left": [6, 12]}}"#,
            &[(5, 13), (5, 12), (4, 7), (2, 6)]
        ),
        [3, 2, 2, 1]
    );
}

#[test]
fn earlier_rounds_take_more_lives() {
    let mode = Knockout::from_json(r#"{"extra_losses": {"before_round": [8, 16]}}"#).unwrap();
    let losses: Vec<usize> = [1, 2, 7, 8, 15, 16, 30]
        .into_iter()
        .map(|round| mode.round_losses(round, 64, 64))
        .collect();
    // Nobody loses a life in the round without elimination.
    assert_eq!(losses, [0, 3, 3, 2, 2, 1, 1]);
    // One player always stays.
    assert_eq!(mode.round_losses(2, 3, 3), 2);
}

#[test]
fn estimates_the_rounds_of_the_match() {
    let mode = Knockout::from_json("").unwrap();
    // 1 round without elimination, 12 rounds down to 16 players, 6 down to 4 and 3
    // down to the winner.
    assert_eq!(mode.rounds(1, 64), 22);
    assert_eq!(mode.rounds(2, 64), 22);
    assert_eq!(mode.rounds(3, 60), 22);
    // Two players need a round with an elimination.
    assert_eq!(mode.rounds(1, 2), 2);
    assert_eq!(mode.rounds(1, 0), 2);
    // Two eliminations per round until round 8 take 10 players down to 2 in
    // round 5, round 6 decides the match.
    let mode = Knockout::from_json(r#"{"extra_losses": {"before_round": [8]}}"#).unwrap();
    assert_eq!(mode.rounds(1, 10), 6);
}

#[test]
fn players_take_part_from_their_first_race() {
    let mut harness = started("");
    assert_eq!(harness.standings.info, "Round 1 / 2, no K.O.");
    assert!(harness.step(race_start("b")).is_empty());
    harness.step(race_start("a"));
    // Listed by name without a position until a round counted for them.
    assert_eq!(
        ranking(&harness),
        [("a", 0, Status::Playing, ""), ("b", 0, Status::Playing, "")]
    );
    assert_eq!(lives(&harness), [("a", 1), ("b", 1)]);
    assert_eq!(harness.standings.info, "Round 1 / 2, 2 left, no K.O.");

    // Players who join in the next round follow the ranked players.
    harness.step(round_end(finishers(&["b", "a"])));
    harness.step(race_start("d"));
    harness.step(race_start("c"));
    assert_eq!(
        ranking(&harness),
        [
            ("b", 1, Status::Playing, "0:30.000"),
            ("a", 2, Status::Playing, "0:31.000"),
            ("c", 0, Status::Playing, ""),
            ("d", 0, Status::Playing, ""),
        ]
    );
}

#[test]
fn the_first_round_eliminates_nobody() {
    let mut harness = started("");
    let commands = harness.step(round_end(vec![
        finish("b", 31_000),
        dnf("c", 2),
        finish("a", 30_000),
        spectator("s"),
    ]));
    assert!(commands.is_empty());
    // Everyone who started takes part, ranked by the race.
    assert_eq!(
        ranking(&harness),
        [
            ("a", 1, Status::Playing, "0:30.000"),
            ("b", 2, Status::Playing, "0:31.000"),
            ("c", 3, Status::Playing, ""),
        ]
    );
    assert_eq!(harness.standings.info, "Round 2 / 3, 3 left, 1 K.O.");
}

#[test]
fn players_join_until_the_first_round_with_eliminations_ended() {
    let mut harness = playing("", &["a", "b", "c", "d"]);
    // e joins in time and takes part. Five players, two are eliminated.
    let mut results = finishers(&["a", "b", "c", "d"]);
    results.extend([finish("e", 29_000), spectator("s")]);
    let commands = harness.step(round_end(results));
    assert_eq!(harness.standing("e").position, 1);
    assert_eq!(blocked(&commands), ["c", "d", "s"]);
    assert_eq!(harness.standing("s").status, Status::Spectator);

    // Too late, f does not take part and cannot spawn anymore.
    harness.step(race_start("f"));
    assert!(harness.standings.player("f").is_none());
    let mut results = finishers(&["e", "a", "b"]);
    results.push(finish("f", 28_000));
    let commands = harness.step(round_end(results));
    assert_eq!(blocked(&commands), ["b", "f"]);
    assert_eq!(harness.standing("f").status, Status::Spectator);
    assert_eq!(harness.standing("f").position, 0);
}

#[test]
fn the_last_players_are_eliminated() {
    let mut harness = playing("", &["a", "b", "c", "d", "e", "f"]);
    // Six players, two are eliminated.
    let commands = harness.step(round_end(finishers(&["b", "a", "c", "d", "e", "f"])));
    assert_eq!(
        chat(&commands),
        [
            "$f90$i$<e$> takes 5th place!",
            "$f90$i$<f$> takes 6th place!"
        ]
    );
    assert_eq!(blocked(&commands), ["e", "f"]);
    assert_eq!(
        ranking(&harness),
        [
            ("b", 1, Status::Playing, "0:30.000"),
            ("a", 2, Status::Playing, "0:31.000"),
            ("c", 3, Status::Playing, "0:32.000"),
            ("d", 4, Status::Playing, "0:33.000"),
            ("e", 5, Status::Eliminated, "K.O."),
            ("f", 6, Status::Eliminated, "K.O."),
        ]
    );
    assert_eq!(harness.standings.info, "Round 3 / 5, 4 left, 1 K.O.");

    // Places follow the race and show who lost their life.
    let places: Vec<(&str, u32, Status, i32)> = harness
        .round
        .iter()
        .map(|p| (p.account_id.as_str(), p.position, p.status, p.score))
        .collect();
    assert_eq!(
        places[3..],
        [
            ("d", 4, Status::Playing, 0),
            ("e", 5, Status::Eliminated, -1),
            ("f", 6, Status::Eliminated, -1)
        ]
    );
}

#[test]
fn everyone_without_a_finish_is_eliminated() {
    let mut harness = playing("", &["a", "b", "c", "d", "e", "f"]);
    // Two eliminations, but three players did not finish. f left the server.
    let commands = harness.step(round_end(vec![
        finish("a", 30_000),
        dnf("c", 2),
        finish("b", 31_000),
        dnf("e", 0),
        finish("d", 29_000),
    ]));
    // By the checkpoints they passed, players who did not start last.
    assert_eq!(
        chat(&commands),
        [
            "$f90$i$<c$> takes 4th place!",
            "$f90$i$<e$> takes 5th place!",
            "$f90$i$<f$> takes 6th place!"
        ]
    );
    let in_match: Vec<&str> = harness
        .standings
        .in_match()
        .map(|p| p.account_id.as_str())
        .collect();
    assert_eq!(in_match, ["d", "a", "b"]);
}

#[test]
fn the_slowest_finishers_fill_up_the_eliminations() {
    let mut harness = playing("", &["a", "b", "c", "d", "e", "f"]);
    let mut results = finishers(&["a", "b", "c", "d", "e"]);
    results.push(dnf("f", 1));
    let commands = harness.step(round_end(results));
    assert_eq!(blocked(&commands), ["e", "f"]);
}

#[test]
fn the_last_player_wins() {
    let mut harness = playing("", &["a", "b"]);
    let commands = harness.step(round_end(finishers(&["b", "a"])));
    assert_eq!(
        chat(&commands),
        ["$f90$i$<a$> takes 2nd place!", "$<b$> wins the match!"]
    );
    assert_eq!(commands.last(), Some(&Command::EndMatch));
    assert_eq!(
        ranking(&harness),
        [
            ("b", 1, Status::Playing, "Winner"),
            ("a", 2, Status::Eliminated, "K.O.")
        ]
    );
    assert_eq!(harness.standings.info, "Match over");
    assert_eq!(harness.step(Event::MapEnd), [Command::EndMatch]);
}

#[test]
fn the_player_who_got_the_furthest_wins_without_finishers() {
    let mut harness = playing("", &["a", "b", "c"]);
    let commands = harness.step(round_end(vec![dnf("a", 1), dnf("b", 3), dnf("c", 2)]));
    assert_eq!(commands.last(), Some(&Command::EndMatch));
    assert_eq!(
        ranking(&harness),
        [
            ("b", 1, Status::Playing, "Winner"),
            ("c", 2, Status::Eliminated, "K.O."),
            ("a", 3, Status::Eliminated, "K.O."),
        ]
    );
}

#[test]
fn a_single_player_wins_once_nobody_can_join() {
    let mut harness = playing("", &["a"]);
    assert_eq!(harness.standings.info, "Round 2 / 2, 1 left, no K.O.");
    let commands = harness.step(round_end(vec![dnf("a", 1)]));
    assert_eq!(
        commands,
        [
            Command::ChatMessage("$<a$> wins the match!".into()),
            Command::EndMatch
        ]
    );
}

#[test]
fn eliminates_from_the_first_round_without_rounds_without_elimination() {
    let mut harness = started(r#"{"rounds_without_elimination": 0}"#);
    assert_eq!(harness.standings.info, "Round 1 / 1, no K.O.");
    let commands = harness.step(round_end(finishers(&["a", "b", "c"])));
    assert_eq!(chat(&commands), ["$f90$i$<c$> takes 3rd place!"]);
}

#[test]
fn equal_times_go_to_the_better_last_checkpoint() {
    let mut harness = playing("", &["a", "b", "c"]);
    // a and b finish in the same time, b passed the last checkpoint earlier.
    let commands = harness.step(round_end(vec![
        finish_at("a", &[10_000, 20_500, 30_000]),
        finish_at("b", &[10_100, 20_400, 30_000]),
        finish("c", 29_000),
    ]));
    assert_eq!(chat(&commands), ["$f90$i$<a$> takes 3rd place!"]);
    assert_eq!(
        ranking(&harness),
        [
            ("c", 1, Status::Playing, "0:29.000"),
            ("b", 2, Status::Playing, "0:30.000"),
            ("a", 3, Status::Eliminated, "K.O."),
        ]
    );
}

#[test]
fn equal_last_checkpoints_go_to_the_one_before() {
    let mut harness = playing("", &["a", "b", "c"]);
    let commands = harness.step(round_end(vec![
        finish_at("a", &[10_000, 20_000, 30_000]),
        finish_at("b", &[9_900, 20_000, 30_000]),
        finish("c", 29_000),
    ]));
    assert_eq!(chat(&commands), ["$f90$i$<a$> takes 3rd place!"]);
}

#[test]
fn equal_races_go_by_name() {
    let mut harness = playing("", &["x", "y", "z"]);
    named(&mut harness, "x", "bob");
    named(&mut harness, "y", "Alice");
    let race = [10_000, 20_000, 30_000];
    let commands = harness.step(round_end(vec![
        finish_at("x", &race),
        finish_at("y", &race),
        finish("z", 29_000),
    ]));
    // Alice comes before bob, whatever the case.
    assert_eq!(chat(&commands), ["$f90$i$<bob$> takes 3rd place!"]);
}

#[test]
fn players_who_got_equally_far_go_by_their_checkpoints() {
    let mut harness = playing("", &["a", "b", "c", "d"]);
    // a and b give up after the second checkpoint, b got there first.
    let commands = harness.step(round_end(vec![
        finish("c", 30_000),
        finish("d", 31_000),
        dnf_at("a", &[10_000, 21_000]),
        dnf_at("b", &[10_500, 20_000]),
    ]));
    assert_eq!(
        chat(&commands),
        [
            "$f90$i$<b$> takes 3rd place!",
            "$f90$i$<a$> takes 4th place!"
        ]
    );
}

#[test]
fn nobody_shares_a_position() {
    let mut harness = playing(r#"{"lives": 3}"#, &["a", "b", "c"]);
    // a and b finish in the same time and keep their lives, b with the better
    // checkpoints.
    harness.step(round_end(vec![
        finish_at("a", &[10_000, 20_500, 30_000]),
        finish_at("b", &[10_100, 20_400, 30_000]),
        finish("c", 31_000),
    ]));
    let positions: Vec<(&str, u32)> = harness
        .standings
        .entries
        .iter()
        .map(|p| (p.account_id.as_str(), p.position))
        .collect();
    assert_eq!(positions, [("b", 1), ("a", 2), ("c", 3)]);
}

#[test]
fn the_round_keeps_the_order_of_the_race() {
    let mut harness = playing(r#"{"lives": 2}"#, &["a", "b", "c", "d", "e"]);
    // Five players, two lose a life per round.
    harness.step(round_end(finishers(&["a", "b", "c", "d", "e"])));
    // c and d finish in the same time, d with the better checkpoints. Both lose a
    // life, which eliminates d.
    let commands = harness.step(round_end(vec![
        finish("a", 30_000),
        finish("b", 31_000),
        finish("e", 32_000),
        finish_at("c", &[11_000, 22_000, 33_000]),
        finish_at("d", &[11_000, 21_000, 33_000]),
    ]));
    assert_eq!(chat(&commands), ["$f90$i$<d$> takes 5th place!"]);
    // The round follows the race although c is still in the match.
    let places: Vec<(&str, u32, i32)> = harness
        .round
        .iter()
        .map(|p| (p.account_id.as_str(), p.position, p.score))
        .collect();
    assert_eq!(
        places,
        [
            ("a", 1, 0),
            ("b", 2, 0),
            ("e", 3, 0),
            ("d", 4, -1),
            ("c", 5, -1)
        ]
    );
    assert_eq!(
        lives(&harness),
        [("a", 2), ("b", 2), ("e", 1), ("c", 1), ("d", 0)]
    );
}

#[test]
fn players_start_with_their_lives() {
    let mut harness = started(r#"{"lives": 3}"#);
    assert_eq!(harness.standings.info, "Round 1, no loss of life");
    harness.step(race_start("a"));
    assert_eq!(
        harness.standing("a").label,
        Some(Label::new(hearts(0, 3), Tone::Neutral))
    );
    assert_eq!(harness.standings.info, "Round 1, 1 left, no loss of life");

    // b only shows up in the results.
    harness.step(round_end(finishers(&["b", "a"])));
    assert_eq!(lives(&harness), [("b", 3), ("a", 3)]);
}

#[test]
fn the_last_players_lose_a_life() {
    let mut harness = playing(r#"{"lives": 3}"#, &["a", "b", "c", "d", "e", "f"]);
    // Six players, two lose a life and keep racing.
    let commands = harness.step(round_end(finishers(&["b", "a", "c", "d", "e", "f"])));
    assert!(commands.is_empty());
    let (all, one_lost) = (hearts(0, 3), hearts(1, 2));
    assert_eq!(
        ranking(&harness),
        [
            ("b", 1, Status::Playing, all.as_str()),
            ("a", 2, Status::Playing, all.as_str()),
            ("c", 3, Status::Playing, all.as_str()),
            ("d", 4, Status::Playing, all.as_str()),
            ("e", 5, Status::Playing, one_lost.as_str()),
            ("f", 6, Status::Playing, one_lost.as_str()),
        ]
    );
    assert_eq!(
        lives(&harness),
        [("b", 3), ("a", 3), ("c", 3), ("d", 3), ("e", 2), ("f", 2)]
    );
    assert_eq!(harness.standings.info, "Round 3, 6 left, 2 lose a life");

    // The round shows who lost a life.
    let losses: Vec<(&str, i32)> = harness
        .round
        .iter()
        .map(|p| (p.account_id.as_str(), p.score))
        .collect();
    assert_eq!(
        losses,
        [("b", 0), ("a", 0), ("c", 0), ("d", 0), ("e", -1), ("f", -1)]
    );
}

#[test]
fn players_are_eliminated_with_their_last_life() {
    let mut harness = playing(r#"{"lives": 2}"#, &["a", "b", "c"]);
    // Three players, one loses a life per round.
    assert!(
        harness
            .step(round_end(finishers(&["a", "b", "c"])))
            .is_empty()
    );
    // The last life is in danger.
    assert_eq!(
        harness.standing("c").label,
        Some(Label::new(hearts(1, 1), Tone::Negative))
    );
    assert_eq!(harness.standings.info, "Round 3, 3 left, 1 loses a life");

    let commands = harness.step(round_end(finishers(&["a", "b", "c"])));
    assert_eq!(chat(&commands), ["$f90$i$<c$> takes 3rd place!"]);
    assert_eq!(blocked(&commands), ["c"]);
    assert_eq!(lives(&harness), [("a", 2), ("b", 2), ("c", 0)]);
}

#[test]
fn eliminated_players_take_the_last_ranks_of_the_players_left() {
    let mut harness = playing(r#"{"lives": 2}"#, &["a", "b", "c", "d", "e"]);
    // Five players, two lose a life per round.
    harness.step(round_end(finishers(&["a", "b", "d", "c", "e"])));
    assert_eq!(
        lives(&harness),
        [("a", 2), ("b", 2), ("d", 2), ("c", 1), ("e", 1)]
    );

    // d finishes behind c, but has a life to spare.
    let commands = harness.step(round_end(finishers(&["a", "b", "e", "c", "d"])));
    assert_eq!(chat(&commands), ["$f90$i$<c$> takes 5th place!"]);
    let (all, last) = (hearts(0, 2), hearts(1, 1));
    assert_eq!(
        ranking(&harness),
        [
            ("a", 1, Status::Playing, all.as_str()),
            ("b", 2, Status::Playing, all.as_str()),
            ("e", 3, Status::Playing, last.as_str()),
            ("d", 4, Status::Playing, last.as_str()),
            ("c", 5, Status::Eliminated, "K.O."),
        ]
    );
}

#[test]
fn a_life_to_spare_outlasts_getting_further() {
    let mut harness = playing(r#"{"lives": 2}"#, &["a", "b"]);
    harness.step(round_end(finishers(&["a", "b"])));
    // Nobody finishes. b got further, but loses the last life.
    let commands = harness.step(round_end(vec![dnf("a", 1), dnf("b", 3)]));
    assert_eq!(
        chat(&commands),
        ["$f90$i$<b$> takes 2nd place!", "$<a$> wins the match!"]
    );
    assert_eq!(lives(&harness), [("a", 1), ("b", 0)]);
}

#[test]
fn the_player_who_got_the_furthest_keeps_the_last_life() {
    let mut harness = playing(r#"{"lives": 2}"#, &["a", "b"]);
    harness.step(round_end(finishers(&["a", "b"])));
    harness.step(round_end(finishers(&["b", "a"])));
    assert_eq!(lives(&harness), [("b", 1), ("a", 1)]);

    // Rather than both being out, a keeps the last life without a finish.
    let commands = harness.step(round_end(vec![dnf("a", 2), dnf("b", 1)]));
    assert_eq!(
        chat(&commands),
        ["$f90$i$<b$> takes 2nd place!", "$<a$> wins the match!"]
    );
    assert_eq!(lives(&harness), [("a", 1), ("b", 0)]);
}

#[test]
fn many_lives_are_counted() {
    let mut harness = playing(r#"{"lives": 6}"#, &["a", "b"]);
    assert_eq!(
        harness.standing("a").label,
        Some(Label::new("6 lives", Tone::Neutral))
    );
    for _ in 0..5 {
        harness.step(round_end(finishers(&["a", "b"])));
    }
    assert_eq!(
        harness.standing("b").label,
        Some(Label::new("1 life", Tone::Negative))
    );
}

#[test]
fn invalid_rounds_do_not_count() {
    let invalid = |results| {
        Event::RoundEnd(RoundEnd {
            invalid: true,
            results,
        })
    };
    let mut harness = playing("", &["a", "b", "c"]);
    // Nobody is eliminated, but starters still join, ranked from their next round.
    let commands = harness.step(invalid(vec![finish("d", 31_000), dnf("a", 1)]));
    assert!(commands.is_empty());
    assert_eq!(harness.standings.in_match().count(), 4);
    assert_eq!(harness.standing("d").position, 0);
    assert_eq!(harness.standings.rounds_on_map, 1);
    assert_eq!(harness.standings.info, "Round 2 / 4, 4 left, 1 K.O.");
}

#[test]
fn rounds_without_players_do_not_close_the_registration() {
    let mut harness = started("");
    for _ in 0..3 {
        harness.step(round_end(vec![spectator("s")]));
    }
    assert!(harness.standings.entries.is_empty());
    assert_eq!(harness.standings.rounds_on_map, 0);

    // Like in the original, the rounds without elimination are over though.
    harness.step(round_end(finishers(&["a", "b"])));
    assert_eq!(harness.standings.in_match().count(), 1);
    assert_eq!(harness.standing("a").label, Some(Label::winner()));
}

#[test]
fn map_ends_after_rounds_per_map() {
    let mut harness = started(r#"{"rounds_per_map": 2}"#);
    let commands = harness.step(round_end(finishers(&["a", "b", "c"])));
    assert!(!commands.contains(&Command::EndMap));
    let commands = harness.step(round_end(finishers(&["a", "b", "c"])));
    assert_eq!(commands.last(), Some(&Command::EndMap));

    // The match goes on with the next map.
    assert!(harness.step(Event::MapEnd).is_empty());
    harness.step(Event::MapStart);
    assert_eq!(harness.standings.rounds_on_map, 0);
    assert_eq!(harness.standings.info, "Round 3 / 3, 2 left, 1 K.O.");
}

#[test]
fn info_follows_the_eliminations() {
    let mut harness = started("");
    let players: Vec<String> = (0..64).map(|i| format!("p{i:02}")).collect();
    let players: Vec<&str> = players.iter().map(String::as_str).collect();
    for player in &players {
        harness.step(race_start(player));
    }
    assert_eq!(harness.standings.info, "Round 1 / 22, 64 left, no K.O.");
    harness.step(round_end(finishers(&players)));
    assert_eq!(harness.standings.info, "Round 2 / 22, 64 left, 4 K.O.");
    harness.step(round_end(finishers(&players)));
    assert_eq!(harness.standings.info, "Round 3 / 22, 60 left, 4 K.O.");
}

#[test]
fn rejects_invalid_settings() {
    assert!(Knockout::from_json(r#"{"rounds_without_eliminations": 1}"#).is_err());
    assert!(Knockout::from_json(r#"{"lives": 0}"#).is_err());
    assert!(Knockout::from_json(r#"{"extra_losses": {"above_players_left": [-1]}}"#).is_err());
    assert!(Knockout::from_json(r#"{"extra_losses": {"above_players": [4]}}"#).is_err());
    assert!(Knockout::from_json(r#"{"extra_losses": [4, 16, 16]}"#).is_err());
    assert!(Knockout::from_json(r#"{"finish_timeout": "automatic"}"#).is_ok());
}

#[test]
fn race_times() {
    let times: Vec<String> = [0, 42_123, 62_005, 3_723_456]
        .into_iter()
        .map(race_time)
        .collect();
    assert_eq!(times, ["0:00.000", "0:42.123", "1:02.005", "62:03.456"]);
}
