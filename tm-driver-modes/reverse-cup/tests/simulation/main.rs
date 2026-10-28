//! Simulated reverse cup matches, checked against the rules of the reverse cup: random
//! ones, full servers and the logs saved in `logs/`. The matches of seeds 1 to 3
//! are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-driver-mode-reverse-cup --test simulation`
//! checks more matches, `SIM_SEED=42` only seed 42 and prints its report.
//! `SIM_SAVE=tests/simulation/logs` saves the shrunk inputs of the first seed that
//! fails, to keep it as a test.

mod rules;

use std::sync::OnceLock;

use serde_json::{Value, json};
use tm_driver_sim::{Rng, Simulator, random};

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("reverse-cup", &tm_driver_sim::wasm!())
            .settings(settings)
            .rules(rules::ReverseCup::boxed)
    })
}

/// Every setting varies, with a repartition that takes points every round.
fn settings(rng: &mut Rng) -> String {
    let mut settings = json!({
        "starting_points": rng.between(5, 60),
        "points_repartition": random::rising_points(rng),
        "fast_forward_points_repartition": rng.chance(500),
        "dnf_points_loss": rng.between(5, 30),
        "disable_last_chance": rng.chance(200),
        "last_chance_dnf_mode": if rng.chance(500) { "all_players" } else { "only_least_checkpoints" },
        "allow_fast_forward_rounds": rng.chance(500),
        // A match with no more players than winners waits for players.
        "number_of_winners": *rng.pick(&[1, 1, 1, 2, 3]),
        "rounds_per_map": rng.between(0, 5),
        "finish_timeout": random::finish_timeout(rng),
    });
    if rng.chance(300) {
        let complex: serde_json::Map<String, Value> = (2..=rng.between(2, 5))
            .map(|left| (left.to_string(), json!(random::rising_points(rng))))
            .collect();
        settings["complex_points_repartition"] = Value::Object(complex);
    } else if rng.chance(300) {
        settings["complex_points_repartition"] = json!({});
    }
    if let Some(warmup) = random::warmup(rng) {
        settings["warmup"] = warmup;
    }
    settings.to_string()
}

tm_driver_sim::simulation_tests!(simulator);
