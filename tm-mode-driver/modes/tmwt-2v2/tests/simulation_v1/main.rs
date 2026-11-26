//! Simulated TMWT 2v2 matches of two teams of two, checked against the rules of the
//! mode: random ones, full servers, which are the same four players, and the logs
//! saved in `logs/`. The matches of seeds 1 to 3 are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-mode-tmwt-2v2 --test simulation_v1`
//! checks more matches, `SIM_SEED=42` only seed 42 and prints its report.

mod rules;

use std::sync::OnceLock;

use serde_json::json;
use tm_mode_driver_sdk::Native;
use tm_mode_driver_sim::{Rng, Simulator, random};
use tm_mode_tmwt_2v2::v1::Tmwt2v2;

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("tmwt 2v2", Native::<Tmwt2v2>::new())
            .teams(2, 2)
            .settings(settings)
            .rules(rules::Tmwt2v2::boxed)
    })
}

/// Short maps and matches now and then, so every limit gets reached.
fn settings(rng: &mut Rng) -> String {
    json!({
        "map_points_limit": rng.between(1, 10),
        "match_points_limit": rng.between(1, 4),
        "finish_timeout": random::finish_timeout(rng),
    })
    .to_string()
}

tm_mode_driver_sim::simulation_tests!(simulator);
