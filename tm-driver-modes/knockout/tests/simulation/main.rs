//! Simulated knockout matches, checked against the rules of the knockout: random
//! ones, full servers and the scenarios of `scenarios/`. The matches of seeds 1 to
//! 3 and the scenarios are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-driver-mode-knockout --test simulation`
//! checks more matches, `SIM_SEED=42` only seed 42 and prints its report.

mod rules;

use std::sync::OnceLock;

use serde_json::json;
use tm_driver_sim::{Rng, Simulator, random, seeds};

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("knockout", &tm_driver_sim::wasm!())
            .settings(settings)
            .rules(rules::Knockout::boxed)
    })
}

/// Lives, the rounds without elimination, the extra losses and the rounds per map.
fn settings(rng: &mut Rng) -> String {
    let milestones: Vec<u32> = (0..rng.below(4)).map(|_| rng.between(0, 16)).collect();
    let extra_losses = match rng.below(3) {
        0 => json!({"above_players_left": milestones}),
        1 => json!({"above_lives_left": milestones}),
        _ => json!({"before_round": milestones}),
    };
    let mut settings = json!({
        "lives": rng.between(1, 3),
        "rounds_without_elimination": rng.between(0, 2),
        "extra_losses": extra_losses,
        "rounds_per_map": rng.between(0, 4),
        "finish_timeout": random::finish_timeout(rng),
    });
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
