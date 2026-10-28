//! Random matches of every mode keep every check. A few of them are kept as
//! snapshots, so a change to a mode or the driver shows up in review.
//!
//! More seeds: `SIM_SEEDS=2000 cargo test -p tm-driver-sim --release --test seeds`.
//! A failing seed: `cargo run -p tm-driver-sim -- <mode> <seed>`.

use tm_driver_sim::{Mode, simulate_seed};

/// Seeds whose matches are kept as snapshots.
const SNAPSHOTS: [u64; 3] = [1, 2, 3];

#[test]
fn snapshots_of_random_matches() {
    for mode in Mode::ALL {
        for seed in SNAPSHOTS {
            let outcome = simulate_seed(mode, seed);
            assert_eq!(outcome.failure, None, "\n{}", outcome.report);
            insta::assert_snapshot!(format!("{}_{seed}", mode.name()), outcome.report);
        }
    }
}

#[test]
fn random_matches_keep_every_check() {
    let seeds: u64 = std::env::var("SIM_SEEDS")
        .ok()
        .and_then(|seeds| seeds.parse().ok())
        .unwrap_or(25);
    // Beyond the snapshots, so the sweep tests other matches.
    let first = 100;

    let failures: Vec<String> = std::thread::scope(|scope| {
        let sweeps: Vec<_> = Mode::ALL
            .into_iter()
            .map(|mode| {
                scope.spawn(move || {
                    (first..first + seeds)
                        .filter_map(|seed| {
                            let failure = simulate_seed(mode, seed).failure?;
                            Some(format!(
                                "{} seed {seed}: {} after input {}\n  cargo run -p tm-driver-sim -- {} {seed}",
                                mode.name(),
                                failure.message,
                                failure.input,
                                mode.name()
                            ))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        sweeps
            .into_iter()
            .flat_map(|sweep| sweep.join().expect("A sweep panicked"))
            .collect()
    });
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
