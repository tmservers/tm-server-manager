//! Simulated time attack matches, checked against the rules of time attack: random
//! ones, full servers and the logs saved in `logs/`. The matches of seeds 1 to 3
//! are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-mode-time-attack --test simulation`
//! checks more matches, `SIM_SEED=42` only seed 42 and prints its report.
//! `SIM_SAVE=tests/simulation/logs` saves the shrunk inputs of the first seed that
//! fails, to keep it as a test.

mod rules;

use std::sync::OnceLock;

use serde_json::json;
use tm_mode_driver_sim::{InputLog, Rng, Simulator, events::Race, random};

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("time-attack", &tm_mode_driver_sim::wasm!())
            .settings(settings)
            .rules(rules::TimeAttack::boxed)
    })
}

/// Short time limits, points by place now and then, maps per match.
fn settings(rng: &mut Rng) -> String {
    let mut settings = json!({"time_limit_ms": rng.between(15, 90) * 1000});
    if rng.chance(500) {
        settings["points_repartition"] = json!(random::falling_points(rng, 10));
    }
    if rng.chance(500) {
        settings["maps_per_match"] = json!(rng.between(1, 3));
    }
    if let Some(warmup) = random::warmup(rng) {
        settings["warmup"] = warmup;
    }
    settings.to_string()
}

tm_mode_driver_sim::simulation_tests!(simulator);

/// The best race of the round counts, not the last one. A race the time limit cuts
/// off has no finish and does not count.
#[test]
fn the_best_race_counts() {
    let mut log = InputLog::new(r#"{"time_limit_ms": 100000}"#);
    log.players(&["a", "b"]).match_start().map_start("m1");
    log.round(&[
        ("a", Race::finish(&[15_000, 31_000])),
        ("a", Race::finish(&[15_000, 30_000])),
        ("a", Race::finish(&[16_000, 35_000])),
        ("b", Race::finish(&[15_000, 30_500])),
        ("b", Race::finish(&[15_000, 31_000])),
        ("b", Race::dnf(&[14_000])),
    ]);
    let outcome = simulator().log("the best race counts", &log);
    assert_eq!(outcome.failure, None, "\n{}", outcome.report);
    insta::assert_snapshot!(outcome.report);
}
