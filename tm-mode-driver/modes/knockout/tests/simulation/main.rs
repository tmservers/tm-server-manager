//! Simulated knockout matches, checked against the rules of the knockout: random
//! ones, full servers and the logs saved in `logs/`. The matches of seeds 1 to 3
//! are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-mode-knockout --test simulation`
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
        Simulator::new("knockout", &tm_mode_driver_sim::wasm!())
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

tm_mode_driver_sim::simulation_tests!(simulator);

/// A log does not follow the driver like a simulation does, so it can describe a
/// match the driver never asked for. That fails instead of testing nonsense.
#[test]
fn a_log_with_an_eliminated_player_racing_fails() {
    let mut log = InputLog::new(r#"{"rounds_without_elimination": 0}"#);
    log.players(&["a", "b", "c"]).match_start().map_start("m1");
    let races = [
        ("a", Race::finish(&[10_000, 30_000])),
        ("b", Race::finish(&[10_000, 31_000])),
        ("c", Race::finish(&[10_000, 32_000])),
    ];
    log.round(&races);
    // c lost the only life, but races again.
    log.round(&races);
    let failure = simulator().log("c races again", &log).failure;
    assert_eq!(
        failure.map(|failure| failure.message).as_deref(),
        Some("c starts a race but cannot spawn")
    );
}
