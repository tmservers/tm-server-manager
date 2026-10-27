//! The host plays the playlist of a match with the rounds mode as a real wasm module.

mod common;

use common::{
    ROUNDS, hold, map_start, match_start, no_warm_up, play, race, round_start, without_published,
};
use tm_server_types::{
    event::DriverSection,
    method::{DriverCommand, DriverFinishTimeout, DriverRoundFormat},
};
use tm_wasm_driver::Driver;

fn rounds(settings: &str) -> Driver {
    common::driver(ROUNDS, settings)
}

fn next_map(uid: &str) -> DriverCommand {
    DriverCommand::SetNextMap(uid.into())
}

/// Plays a map of one round and returns the commands of its map start and map end holds.
fn play_map(driver: &mut Driver, uid: &str, seq: u32) -> (Vec<DriverCommand>, Vec<DriverCommand>) {
    driver.handle(&map_start(uid)).unwrap();
    let map_start = driver.handle(&hold(seq, DriverSection::MapStart)).unwrap();
    driver.handle(&round_start(1)).unwrap();
    play(driver, &race("a", 30_000));
    driver
        .handle(&hold(seq + 1, DriverSection::RoundEnd))
        .unwrap();
    let map_end = driver
        .handle(&hold(seq + 2, DriverSection::MapEnd))
        .unwrap();
    (
        without_published(&map_start.commands),
        without_published(&map_end.commands),
    )
}

#[test]
fn plays_the_playlist_in_order_once() {
    let mut driver = rounds(r#"{"rounds_per_map": 1}"#);
    driver.set_playlist(["m1", "m2", "m3"]);
    driver.handle(&match_start()).unwrap();

    let (map_start, map_end) = play_map(&mut driver, "m1", 1);
    assert_eq!(
        map_start,
        [
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt(
                DriverFinishTimeout::Automatic
            )),
            no_warm_up(),
            next_map("m2"),
            DriverCommand::Continue(1)
        ]
    );
    assert_eq!(map_end, [DriverCommand::Continue(3)]);

    let (map_start, _) = play_map(&mut driver, "m2", 4);
    assert!(map_start.contains(&next_map("m3")));
    let standings = driver.standings();
    assert_eq!(standings.map_name, "Map m2");
    assert_eq!(standings.map_count, 3);

    // The playlist wraps around, but a new match starts with its first map anyway.
    let (map_start, map_end) = play_map(&mut driver, "m3", 7);
    assert!(map_start.contains(&next_map("m1")));
    assert_eq!(
        map_end,
        [
            DriverCommand::EndMatch,
            next_map("m1"),
            DriverCommand::Continue(9)
        ]
    );
}

#[test]
fn a_match_that_ends_early_starts_over_with_the_first_map() {
    let mut driver = rounds(r#"{"points_limit": 10}"#);
    driver.set_playlist(["m1", "m2", "m3"]);
    driver.handle(&match_start()).unwrap();
    driver.handle(&map_start("m1")).unwrap();
    driver.handle(&hold(1, DriverSection::MapStart)).unwrap();

    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));
    let round_end = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();
    assert_eq!(
        without_published(&round_end.commands),
        [
            DriverCommand::EndMatch,
            next_map("m1"),
            DriverCommand::Continue(2)
        ]
    );
}

#[test]
fn maps_outside_the_playlist_continue_with_its_first_map() {
    let mut driver = rounds("");
    driver.set_playlist(["m1", "m2"]);
    driver.handle(&match_start()).unwrap();
    driver.handle(&map_start("other")).unwrap();
    let map_start = driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    assert!(map_start.commands.contains(&next_map("m1")));
}

#[test]
fn without_a_playlist_the_server_decides() {
    let mut driver = rounds("");
    driver.handle(&match_start()).unwrap();
    driver.handle(&map_start("m1")).unwrap();
    let map_start = driver.handle(&hold(1, DriverSection::MapStart)).unwrap();
    assert!(
        !map_start
            .commands
            .iter()
            .any(|c| matches!(c, DriverCommand::SetNextMap(_)))
    );
    assert_eq!(driver.standings().map_count, 0);
}
