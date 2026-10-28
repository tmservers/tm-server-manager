//! Simulated time attack matches, checked against the rules of time attack:
//! random ones, full servers and the scenarios of `scenarios/`. The matches of
//! seeds 1 to 3 and the scenarios are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-driver-mode-time-attack --test simulation`
//! checks more matches, `SIM_SEED=42` only seed 42 and prints its report.

mod rules;

use std::sync::OnceLock;

use serde_json::json;
use tm_driver_sim::{Rng, Simulator, random, seeds};

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("time-attack", &tm_driver_sim::wasm!())
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

#[test]
fn random_matches_follow_the_rules() {
    simulator().check(seeds("SIM_SEEDS", 100..125), Simulator::world);
}

#[test]
fn full_servers_follow_the_rules() {
    simulator().check(seeds("SIM_FULL_SERVERS", 100..102), Simulator::full_server);
}

#[test]
fn snapshots_of_random_matches() {
    for seed in 1..=3 {
        let outcome = simulator().seed(seed);
        assert_eq!(outcome.failure, None, "\n{}", outcome.report);
        insta::assert_snapshot!(format!("seed_{seed}"), outcome.report);
    }
}

#[test]
fn scenarios() {
    insta::glob!("scenarios/*.scn", |path| {
        let scenario = std::fs::read_to_string(path).unwrap();
        let outcome = simulator().scenario(&scenario).unwrap();
        assert_eq!(outcome.failure, None, "\n{}", outcome.report);
        insta::assert_snapshot!(outcome.report);
    });
}
