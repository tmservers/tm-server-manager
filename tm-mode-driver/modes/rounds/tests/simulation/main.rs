//! Simulated rounds matches, checked against the rules of the rounds mode: random
//! ones, full servers and the logs saved in `logs/`. The matches of seeds 1 to 3
//! are kept as snapshots.
//!
//! `SIM_SEEDS=5000 cargo test --release -p tm-mode-rounds --test simulation`
//! checks more matches, `SIM_SEED=42` only seed 42 and prints its report.
//! `SIM_SAVE=tests/simulation/logs` saves the shrunk inputs of the first seed that
//! fails, to keep it as a test.

mod rules;

use std::sync::OnceLock;

use serde_json::json;
use tm_mode_driver_script::ScriptMode;
use tm_mode_driver_sdk::Native;
use tm_mode_driver_sim::{
    InputLog, Rng, Simulator,
    events::Race,
    random,
    rules::{RoundEnd, Rules},
};
use tm_mode_rounds::Rounds;

fn simulator() -> &'static Simulator {
    static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
    SIMULATOR.get_or_init(|| {
        Simulator::new("rounds", Native::<Rounds>::new())
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

tm_mode_driver_sim::simulation_tests!(simulator);

/// The rounds mode as a competition would write it as a script.
const SCRIPT: &str = include_str!("../../rounds.rhai");

/// The report of a simulation without what its calls cost, which built-in modes do not count.
fn without_cost(report: &str) -> String {
    report
        .lines()
        .filter(|line| !line.starts_with("cost "))
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn the_script_plays_the_matches_of_the_built_in_mode() {
    let script = Simulator::new("rounds", ScriptMode::new(SCRIPT).unwrap())
        .settings(settings)
        .rules(rules::Rounds::boxed);
    let worlds: [(_, fn(&Simulator, u64) -> tm_mode_driver_sim::World); 2] = [
        (
            tm_mode_driver_sim::seeds("SIM_SEEDS", 100..125),
            Simulator::world,
        ),
        (
            tm_mode_driver_sim::seeds("SIM_FULL_SERVERS", 100..102),
            Simulator::full_server,
        ),
    ];

    for (seeds, world) in worlds {
        for seed in seeds {
            let built_in = simulator().simulate(&world(simulator(), seed), seed);
            let scripted = script.simulate(&world(&script, seed), seed);

            assert_eq!(scripted.failure, None, "seed {seed}\n{}", scripted.report);
            // Every round, every standing and every command the driver sent.
            assert_eq!(
                without_cost(&scripted.report),
                built_in.report,
                "the script plays seed {seed} differently"
            );
            assert!(
                scripted
                    .report
                    .lines()
                    .any(|line| line.starts_with("cost ")),
                "the calls of a script are counted"
            );
        }
    }
}

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
    let simulator =
        Simulator::new("rounds", Native::<Rounds>::new()).rules(|_| Ok(Box::new(BMustNotWin)));
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
