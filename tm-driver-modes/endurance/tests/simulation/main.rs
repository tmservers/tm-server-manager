//! Simulated endurance races, checked against the rules of the endurance mode:
//! random ones, full servers and the logs saved in `logs/`. The races of seeds 1 to
//! 3 are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-driver-mode-endurance --test simulation`
//! checks more races, `SIM_SEED=42` only seed 42 and prints its report.
//! `SIM_SAVE=tests/simulation/logs` saves the shrunk inputs of the first seed that
//! fails, to keep it as a test.

mod rules;

use std::sync::OnceLock;

use serde_json::json;
use tm_driver_sim::{InputLog, Rng, Simulator, events::Race, random};

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("endurance", &tm_driver_sim::wasm!())
            .settings(settings)
            .rules(rules::Endurance::boxed)
    })
}

/// Infinite laps with a short time limit or a few laps with a finish timeout.
fn settings(rng: &mut Rng) -> String {
    let mut settings = if rng.chance(500) {
        json!({"time_limit_ms": rng.between(20, 120) * 1000})
    } else {
        json!({
            "laps": rng.between(1, 5),
            "finish_timeout": random::finish_timeout(rng),
        })
    };
    if let Some(warmup) = random::warmup(rng) {
        settings["warmup"] = warmup;
    }
    settings.to_string()
}

tm_driver_sim::simulation_tests!(simulator);

/// With infinite laps on a map whose laps have three checkpoints, the time limit
/// cuts off every race. The most checkpoints win, b got as far as a but earlier.
#[test]
fn the_most_checkpoints_win() {
    let mut log = InputLog::new(r#"{"time_limit_ms": 60000}"#);
    log.players(&["a", "b", "c"]).match_start().map_start("m1");
    log.round(&[
        (
            "a",
            Race::dnf(&[8_000, 17_000, 25_000, 33_000, 42_000, 50_000, 58_000]).in_laps(3),
        ),
        (
            "b",
            Race::dnf(&[7_000, 16_000, 24_000, 32_000, 41_000, 49_000, 57_000]).in_laps(3),
        ),
        (
            "c",
            Race::dnf(&[9_000, 19_000, 30_000, 41_000, 52_000]).in_laps(3),
        ),
    ]);
    let outcome = simulator().log("the most checkpoints win", &log);
    assert_eq!(outcome.failure, None, "\n{}", outcome.report);
    insta::assert_snapshot!(outcome.report);
}
