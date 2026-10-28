//! Simulated rounds matches, checked against the rules of the rounds mode: random
//! ones, full servers and the logs saved in `logs/`. The matches of seeds 1 to 3
//! are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-driver-mode-rounds --test simulation`
//! checks more matches, `SIM_SEED=42` only seed 42 and prints its report.
//! `SIM_SAVE=tests/simulation/logs` saves the shrunk inputs of the first seed that
//! fails, to keep it as a test.

mod rules;

use std::sync::OnceLock;

use serde_json::json;
use tm_driver_sim::{
    InputLog, Rng, Simulator,
    events::Race,
    random,
    rules::{RoundEnd, Rules},
};

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("rounds", &tm_driver_sim::wasm!())
            .settings(settings)
            .rules(rules::Rounds::boxed)
    })
}

/// Points by place, a points limit or rounds per map, maps per match.
fn settings(rng: &mut Rng) -> String {
    let points_limit = if rng.chance(300) {
        rng.between(10, 40)
    } else {
        0
    };
    // Without a points limit the map needs an end.
    let rounds_per_map = rng.between(u32::from(points_limit == 0), 5);
    let mut settings = json!({
        "points_repartition": random::falling_points(rng, 15),
        "rounds_per_map": rounds_per_map,
        "points_limit": points_limit,
        "finish_timeout": random::finish_timeout(rng),
    });
    if rng.chance(500) {
        settings["maps_per_match"] = json!(rng.between(1, 3));
    }
    if let Some(warmup) = random::warmup(rng) {
        settings["warmup"] = warmup;
    }
    settings.to_string()
}

tm_driver_sim::simulation_tests!(simulator);

/// Fails once b wins a round, to see what shrinking keeps.
struct BMustNotWin;

impl Rules for BMustNotWin {
    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        match end.round.entries.first() {
            Some(winner) if winner.account_id == "b" => Err("b won a round".into()),
            _ => Ok(()),
        }
    }
}

#[test]
fn shrinking_keeps_only_what_fails() {
    let simulator = Simulator::new("rounds", &tm_driver_sim::wasm!())
        .rules(|_| Ok(Box::new(BMustNotWin)));
    let mut log = InputLog::new("{}");
    log.players(&["a", "b", "c"]).match_start().map_start("m1");
    let a_wins = [
        ("a", Race::finish(&[10_000, 30_000])),
        ("b", Race::finish(&[10_000, 31_000])),
        ("c", Race::dnf(&[12_000])),
    ];
    for _ in 0..4 {
        log.round(&a_wins);
    }
    log.round(&[
        ("a", Race::finish(&[10_000, 30_000])),
        ("b", Race::finish(&[10_000, 29_000])),
    ]);
    log.round(&a_wins);

    let shrunk = simulator.shrink(&log);
    let failure = simulator.log("shrunk", &shrunk).failure;
    assert_eq!(
        failure.map(|failure| failure.message).as_deref(),
        Some("b won a round")
    );
    // The round start, the finish of b and the round end are all it takes.
    assert!(shrunk.inputs.len() <= 4, "{:#?}", shrunk.inputs);
}
