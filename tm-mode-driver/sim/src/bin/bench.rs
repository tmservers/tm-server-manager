//! What the modes cost on a full server: simulates a match of every mode with 120
//! players and prints, per kind of call, the fuel the mode used and the time the host
//! spent, and how long replaying the match takes, like a bridge that resumes.
//!
//! ```sh
//! cargo run --release -p tm-mode-driver-sim --bin bench              # every scenario
//! cargo run --release -p tm-mode-driver-sim --bin bench -- endurance # only these
//! BENCH_ENDURANCE_MINUTES=30 cargo run --release -p tm-mode-driver-sim --bin bench -- endurance
//! ```
//!
//! Fuel is the same on every machine, the times are of this one. The simulation
//! itself takes longer than the driver: its checks run after every input and it
//! replays the match twice to check it.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use tm_mode_driver::DriverLimits;
use tm_mode_driver_sim::{Faults, MapSpec, Races, Simulator, World, build_wasm};

/// A match to measure.
struct Scenario {
    name: &'static str,
    package: &'static str,
    settings: String,
    /// What the match is about, for the output.
    about: String,
    world: fn(String) -> World,
}

fn main() {
    let minutes: u32 = std::env::var("BENCH_ENDURANCE_MINUTES")
        .ok()
        .and_then(|minutes| minutes.parse().ok())
        .unwrap_or(120);
    let scenarios = [
        Scenario {
            name: "rounds",
            package: "tm-mode-rounds",
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "time-attack",
            package: "tm-mode-time-attack",
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "reverse-cup",
            package: "tm-mode-reverse-cup",
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "knockout",
            package: "tm-mode-knockout",
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "endurance",
            package: "tm-mode-endurance",
            settings: format!(r#"{{"time_limit_ms": {}}}"#, minutes * 60_000),
            about: format!("{minutes} minutes of infinite laps, 40 s a lap of 4 checkpoints"),
            world: endurance,
        },
    ];

    let wanted: Vec<String> = std::env::args().skip(1).collect();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = workspace.join("target").join("bench");
    for scenario in scenarios
        .iter()
        .filter(|scenario| wanted.is_empty() || wanted.iter().any(|w| w == scenario.name))
    {
        run(scenario, &workspace, &target);
    }
}

fn run(scenario: &Scenario, workspace: &Path, target: &Path) {
    let wasm = build_wasm(scenario.package, workspace, target);
    let simulator = Simulator::new(scenario.name, &wasm);
    let world = (scenario.world)(scenario.settings.clone());

    let started = Instant::now();
    let outcome = simulator.simulate(&world, 1);
    let simulated = started.elapsed();
    if let Some(failure) = &outcome.failure {
        println!(
            "{}: FAILED after input {}: {}\n",
            scenario.name, failure.input, failure.message
        );
        return;
    }
    let started = Instant::now();
    let replayed = simulator.replay(&scenario.settings, &outcome.inputs);
    let replay = started.elapsed();
    if let Err(error) = replayed {
        println!("{}: the replay failed: {error}\n", scenario.name);
        return;
    }

    println!(
        "{} · {} players · {} · {} rounds · {} inputs · simulated in {} · replayed in {}",
        scenario.name,
        world.players.len(),
        scenario.about,
        outcome
            .stats
            .calls
            .get("round-end")
            .map_or(0, |calls| calls.count),
        outcome.inputs.len(),
        seconds(simulated),
        seconds(replay)
    );
    let budget = DriverLimits::default().fuel_per_call;
    println!(
        "  {:<16} {:>6} {:>10} {:>10} {:>8} {:>10} {:>10} {:>9} {:>9} {:>7} {:>11}",
        "call",
        "count",
        "fuel mean",
        "fuel max",
        "budget",
        "call mean",
        "call max",
        "prepare",
        "publish",
        "racers",
        "checkpoints"
    );
    for (kind, calls) in &outcome.stats.calls {
        println!(
            "  {:<16} {:>6} {:>10} {:>10} {:>7.3}% {:>10} {:>10} {:>9} {:>9} {:>7} {:>11}",
            kind,
            calls.count,
            calls.mean_fuel(),
            calls.max_fuel,
            calls.max_fuel as f64 / budget as f64 * 100.,
            micros(calls.mean_call()),
            micros(calls.max_call),
            seconds(calls.prepare),
            seconds(calls.publish),
            calls.max_racers,
            calls.max_checkpoints
        );
    }
    println!();
}

/// A random world with 120 players, without faults so the numbers are only the match.
fn full_server(settings: String) -> World {
    let mut world = World::full_server(1, settings);
    world.faults = Faults::default();
    world
}

/// One long race on a map with laps, everyone there from the start.
fn endurance(settings: String) -> World {
    let mut world = full_server(settings);
    world.maps = vec![MapSpec {
        uid: "m1".into(),
        sectors: vec![10_000; 4],
        laps: 3,
    }];
    world.playlist = Vec::new();
    world.races = Races { ties: 0, idle: 0 };
    world.matches = 1;
    for player in &mut world.players {
        player.joins = None;
        player.leaves = None;
        player.watches = false;
    }
    world
}

fn seconds(duration: Duration) -> String {
    format!("{:.2} s", duration.as_secs_f64())
}

fn micros(duration: Duration) -> String {
    match duration.as_micros() {
        micros @ 0..10_000 => format!("{micros} µs"),
        micros => format!("{:.1} ms", micros as f64 / 1e3),
    }
}
