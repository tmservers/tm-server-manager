//! What the modes cost on a full server: simulates a match of every mode with 120
//! players and prints, per kind of call, the time the host spent and for scripts
//! the operations they ran, and how long replaying the match takes, like a bridge
//! that resumes.
//!
//! ```sh
//! cargo run --release -p tm-mode-driver-runner --bin bench              # every scenario
//! cargo run --release -p tm-mode-driver-runner --bin bench -- endurance # only these
//! BENCH_ENDURANCE_MINUTES=30 cargo run --release -p tm-mode-driver-runner --bin bench -- endurance
//! ```
//!
//! `rounds-script` plays the match of `rounds` with the mode as a script, to compare
//! the engines. The operations of a script are the same on every machine, the times
//! are of this one. The simulation itself takes longer than the driver: its checks
//! run after every input and it replays the match twice to check it.

use std::time::{Duration, Instant};

use tm_mode_driver_script::ScriptMode;
use tm_mode_driver_sim::{Faults, MapSpec, Races, Simulator, World};

/// The rounds mode as a script.
const ROUNDS_SCRIPT: &str = include_str!("../../../modes/rounds/rounds.rhai");

/// A match to measure.
struct Scenario {
    name: &'static str,
    /// The built-in mode, a script without one.
    mode: Option<&'static str>,
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
            mode: Some("rounds"),
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "rounds-script",
            mode: None,
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "time-attack",
            mode: Some("time-attack"),
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "reverse-cup",
            mode: Some("reverse-cup"),
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "knockout",
            mode: Some("knockout"),
            settings: "{}".into(),
            about: "the default settings".into(),
            world: full_server,
        },
        Scenario {
            name: "endurance",
            mode: Some("endurance"),
            settings: format!(r#"{{"time_limit_ms": {}}}"#, minutes * 60_000),
            about: format!("{minutes} minutes of infinite laps, 40 s a lap of 4 checkpoints"),
            world: endurance,
        },
    ];

    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for scenario in scenarios
        .iter()
        .filter(|scenario| wanted.is_empty() || wanted.iter().any(|w| w == scenario.name))
    {
        run(scenario);
    }
}

fn run(scenario: &Scenario) {
    let simulator = match scenario.mode {
        Some(name) => Simulator::new(
            scenario.name,
            tm_mode_driver_builtin::builtin(name).expect("Every scenario names a built-in mode"),
        ),
        None => Simulator::new(
            scenario.name,
            ScriptMode::new(ROUNDS_SCRIPT).expect("The rounds script compiles"),
        ),
    };
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
    // Built-in modes run without a budget.
    let budget = scenario
        .mode
        .is_none()
        .then_some(tm_mode_driver_script::BUDGET);
    println!(
        "  {:<16} {:>6} {:>10} {:>10} {:>8} {:>10} {:>10} {:>9} {:>9} {:>7} {:>11}",
        "call",
        "count",
        "cost mean",
        "cost max",
        "budget",
        "call mean",
        "call max",
        "prepare",
        "publish",
        "racers",
        "checkpoints"
    );
    for (kind, calls) in &outcome.stats.calls {
        let of_budget = match budget {
            Some(budget) => format!("{:.3}%", calls.max_cost as f64 / budget as f64 * 100.),
            None => "-".into(),
        };
        println!(
            "  {:<16} {:>6} {:>10} {:>10} {:>8} {:>10} {:>10} {:>9} {:>9} {:>7} {:>11}",
            kind,
            calls.count,
            calls.mean_cost(),
            calls.max_cost,
            of_budget,
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
