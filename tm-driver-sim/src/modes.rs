//! The first-party modes: their wasm, built and compiled once per process, and
//! random settings for them.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
};

use serde_json::{Value, json};
use tm_wasm_driver::{DriverEngine, DriverLimits};
use wasmtime::component::Component;

use crate::Rng;

/// A mode of `tm-driver-modes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mode {
    Rounds,
    TimeAttack,
    ReverseCup,
    Knockout,
}

impl Mode {
    pub const ALL: [Mode; 4] = [
        Mode::Rounds,
        Mode::TimeAttack,
        Mode::ReverseCup,
        Mode::Knockout,
    ];

    /// The name of its crate without the `tm-driver-mode-` prefix.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Rounds => "rounds",
            Mode::TimeAttack => "time-attack",
            Mode::ReverseCup => "reverse-cup",
            Mode::Knockout => "knockout",
        }
    }

    pub fn from_name(name: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|mode| mode.name() == name)
    }

    fn package(self) -> String {
        format!("tm-driver-mode-{}", self.name())
    }

    /// The driver engine and the compiled mode, built and compiled once per process.
    pub fn compiled(self) -> (&'static DriverEngine, Component) {
        static ENGINE: OnceLock<DriverEngine> = OnceLock::new();
        static COMPILED: Mutex<BTreeMap<Mode, Component>> = Mutex::new(BTreeMap::new());

        let engine = ENGINE.get_or_init(|| {
            DriverEngine::new(DriverLimits::default()).expect("The driver engine could not start")
        });
        // Held while compiling, so simulations running in parallel wait for the first one.
        let mut compiled = COMPILED.lock().unwrap_or_else(|e| e.into_inner());
        let component = compiled.entry(self).or_insert_with(|| {
            engine
                .compile(&wasm()[&self])
                .unwrap_or_else(|e| panic!("{} could not be compiled: {e}", self.package()))
        });
        (engine, component.clone())
    }

    /// Random settings for a match, within the ranges the mode accepts and so that
    /// matches end. Varying them per seed tests combinations nobody would write down.
    pub fn random_settings(self, rng: &mut Rng) -> String {
        let mut settings = match self {
            Mode::Rounds => {
                let points_limit = if rng.chance(300) {
                    rng.between(10, 40)
                } else {
                    0
                };
                // Without a points limit the map needs an end.
                let rounds_per_map = rng.between(u32::from(points_limit == 0), 5);
                let mut settings = json!({
                    "points_repartition": falling(rng, 15),
                    "rounds_per_map": rounds_per_map,
                    "points_limit": points_limit,
                });
                if rng.chance(500) {
                    settings["maps_per_match"] = json!(rng.between(1, 3));
                }
                settings
            }
            Mode::TimeAttack => {
                let mut settings = json!({"time_limit_ms": rng.between(15, 90) * 1000});
                if rng.chance(500) {
                    settings["points_repartition"] = falling(rng, 10).into();
                }
                if rng.chance(500) {
                    settings["maps_per_match"] = json!(rng.between(1, 3));
                }
                settings
            }
            Mode::ReverseCup => {
                let mut settings = json!({
                    "starting_points": rng.between(5, 60),
                    "points_repartition": rising(rng),
                    "fast_forward_points_repartition": rng.chance(500),
                    "dnf_points_loss": rng.between(5, 30),
                    "disable_last_chance": rng.chance(200),
                    "last_chance_dnf_mode": if rng.chance(500) { "all_players" } else { "only_least_checkpoints" },
                    "allow_fast_forward_rounds": rng.chance(500),
                    // A match with no more players than winners waits for players.
                    "number_of_winners": *rng.pick(&[1, 1, 1, 2, 3]),
                    "rounds_per_map": rng.between(0, 5),
                });
                if rng.chance(300) {
                    let mut complex = serde_json::Map::new();
                    for left in 2..=rng.between(2, 5) {
                        complex.insert(left.to_string(), rising(rng).into());
                    }
                    settings["complex_points_repartition"] = Value::Object(complex);
                } else if rng.chance(300) {
                    settings["complex_points_repartition"] = json!({});
                }
                settings
            }
            Mode::Knockout => {
                let milestones: Vec<u32> = (0..rng.below(4)).map(|_| rng.between(0, 16)).collect();
                let extra_losses = match rng.below(3) {
                    0 => json!({"above_players_left": milestones}),
                    1 => json!({"above_lives_left": milestones}),
                    _ => json!({"before_round": milestones}),
                };
                json!({
                    "lives": rng.between(1, 3),
                    "rounds_without_elimination": rng.between(0, 2),
                    "extra_losses": extra_losses,
                    "rounds_per_map": rng.between(0, 4),
                })
            }
        };

        if self != Mode::TimeAttack {
            settings["finish_timeout"] = match rng.below(3) {
                0 => json!("automatic"),
                1 => json!("disabled"),
                _ => json!({"milliseconds": rng.between(1, 15) * 1000}),
            };
        }
        if rng.chance(300) {
            let mut warmup = json!({
                "rounds": rng.between(1, 2),
                "duration_ms": if rng.chance(500) { 0 } else { rng.between(10, 40) * 1000 },
            });
            if rng.chance(500) {
                warmup["timeout_ms"] = json!(rng.between(2, 10) * 1000);
            }
            settings["warmup"] = warmup;
        }
        settings.to_string()
    }
}

/// Points by position: one to six values, each at most the one before.
fn falling(rng: &mut Rng, max: u32) -> Vec<u32> {
    let mut points = rng.between(1, max);
    (0..rng.between(1, 6))
        .map(|_| {
            let value = points;
            points = points.saturating_sub(rng.below(4));
            value
        })
        .collect()
}

/// Points lost by position: one to eight values, each at least the one before and
/// the last above 0, so every round costs points.
fn rising(rng: &mut Rng) -> Vec<u32> {
    let mut points = rng.below(4);
    (0..rng.between(1, 8))
        .map(|_| {
            points += rng.between(1, 5);
            points
        })
        .collect()
}

/// The wasm modules of every mode, built once per process.
fn wasm() -> &'static BTreeMap<Mode, Vec<u8>> {
    static BUILT: OnceLock<BTreeMap<Mode, Vec<u8>>> = OnceLock::new();
    BUILT.get_or_init(|| {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        // A target directory of its own, so the build does not wait for the cargo that
        // runs the tests.
        let target_dir: PathBuf = workspace.join("target").join("tm-driver-sim");
        let mut command = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        command.current_dir(&workspace).args(["build", "--release"]);
        for mode in Mode::ALL {
            command.args(["-p", &mode.package()]);
        }
        let status = command
            .args(["--target", "wasm32-unknown-unknown", "--target-dir"])
            .arg(&target_dir)
            .status()
            .expect("cargo could not be started");
        assert!(status.success(), "Building the modes failed");

        Mode::ALL
            .into_iter()
            .map(|mode| {
                let file = target_dir
                    .join("wasm32-unknown-unknown/release")
                    .join(format!("{}.wasm", mode.package().replace('-', "_")));
                let wasm = std::fs::read(&file)
                    .unwrap_or_else(|_| panic!("{} was not built", file.display()));
                (mode, wasm)
            })
            .collect()
    })
}
