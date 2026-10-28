//! Deterministic simulation testing for the wasm driver and its modes.
//!
//! A simulation runs the real host with a real wasm mode against a fake server.
//! The fake server plays the driver modescript and its players: it sends the
//! callbacks the driver handles, as [`DriverInput`]s, and follows the commands the
//! driver answers with, so the driver steers the match like on a real server.
//! Everything else, players, their races, time and what goes wrong, comes from a
//! [`World`]: random from a seed, or written down as a scenario (see
//! [`scenario`]).
//!
//! Nothing depends on the wall clock or the order threads run in, so a seed or a
//! scenario always gives the same simulation. After every input, checks test what
//! has to hold for every mode, and at the end of every round and map the
//! [`Rules`] of the mode. Once the simulation ended, replaying every input has to
//! give the same driver, and a driver resumed at a random input has to answer the
//! rest the same way. The report of a simulation describes the match, e.g. for
//! snapshots.
//!
//! The simulator knows no mode. A mode tests itself with it in its own tests:
//!
//! ```ignore
//! fn simulator() -> &'static Simulator {
//!     static SIMULATOR: OnceLock<Simulator> = OnceLock::new();
//!     SIMULATOR.get_or_init(|| {
//!         Simulator::new("knockout", &tm_driver_sim::wasm!())
//!             .settings(random_settings)
//!             .rules(rules::Knockout::boxed)
//!     })
//! }
//!
//! #[test]
//! fn random_matches_follow_the_rules() {
//!     simulator().check(tm_driver_sim::seeds("SIM_SEEDS", 100..125), Simulator::world);
//! }
//! ```

mod check;
pub mod random;
mod report;
mod rng;
pub mod rules;
pub mod scenario;
mod server;
mod wasm;
mod world;

use std::{ops::Range, sync::OnceLock, time::Duration};

pub use rng::Rng;
pub use rules::Rules;
use tm_wasm_driver::{Component, Driver, DriverEngine, DriverInput, DriverLimits, DriverReply};
pub use wasm::build_wasm;
pub use world::{
    Action, Faults, MAX_PLAYERS, MapSpec, Moment, PlayerSpec, RacePlan, Races, ScriptedRound, World,
};

use crate::{
    check::Checker,
    report::Report,
    rules::NoRules,
    server::{Server, Step},
};

/// How a simulation went.
pub struct Outcome {
    /// What happened as text: one block per round with the races, the places and
    /// the standings after it.
    pub report: String,
    /// Everything the driver got, e.g. to replay it.
    pub inputs: Vec<DriverInput>,
    /// The first check that failed.
    pub failure: Option<Failure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The input after which the check failed, counted from 0.
    pub input: usize,
    pub message: String,
}

/// A mode to simulate: its wasm, random settings for it and its rules.
pub struct Simulator {
    name: String,
    component: Component,
    settings: fn(&mut Rng) -> String,
    rules: fn(&str) -> Result<Box<dyn Rules>, String>,
}

impl Simulator {
    /// A mode by the name the reports show and its wasm, e.g. from [`wasm!`]. It
    /// plays with its default settings and only has to keep what holds for every
    /// mode, until [`Simulator::settings`] and [`Simulator::rules`] tell more.
    pub fn new(name: impl Into<String>, wasm: &[u8]) -> Self {
        let name = name.into();
        let component = engine()
            .compile(wasm)
            .unwrap_or_else(|e| panic!("{name} could not be compiled: {e}"));
        Self {
            name,
            component,
            settings: |_| "{}".into(),
            rules: |_| Ok(Box::new(NoRules)),
        }
    }

    /// Random settings for the matches of random worlds, within the ranges the mode
    /// accepts and so that matches end. Varying them tests combinations nobody would
    /// write down.
    pub fn settings(mut self, settings: fn(&mut Rng) -> String) -> Self {
        self.settings = settings;
        self
    }

    /// The rules of the mode for a match with these settings.
    pub fn rules(mut self, rules: fn(&str) -> Result<Box<dyn Rules>, String>) -> Self {
        self.rules = rules;
        self
    }

    /// The random world of a seed.
    pub fn world(&self, seed: u64) -> World {
        World::random(seed, self.random_settings(seed))
    }

    /// The random world of a seed on a full server.
    pub fn full_server(&self, seed: u64) -> World {
        World::full_server(seed, self.random_settings(seed))
    }

    fn random_settings(&self, seed: u64) -> String {
        (self.settings)(&mut Rng::new(seed).fork("settings"))
    }

    /// Simulates the random world of a seed.
    pub fn seed(&self, seed: u64) -> Outcome {
        self.simulate(&self.world(seed), seed)
    }

    /// Simulates a scenario, see [`scenario`].
    pub fn scenario(&self, text: &str) -> Result<Outcome, String> {
        let world = scenario::parse(text)?;
        Ok(self.run(&world, 0, format!("{} · scenario", self.name)))
    }

    /// Simulates a world. The seed decides what the world leaves open, e.g. the
    /// order of callbacks at the same moment.
    pub fn simulate(&self, world: &World, seed: u64) -> Outcome {
        self.run(world, seed, format!("{} · seed {seed}", self.name))
    }

    /// Simulates the worlds of these seeds in parallel and panics with every failure
    /// and the report of the first one. A single seed prints its report.
    pub fn check(&self, seeds: Range<u64>, world: fn(&Simulator, u64) -> World) {
        if seeds.end - seeds.start == 1 {
            let outcome = self.simulate_caught(world, seeds.start);
            println!("{}", outcome.report);
            if let Some(failure) = &outcome.failure {
                panic!(
                    "seed {}: {} after input {}",
                    seeds.start, failure.message, failure.input
                );
            }
            return;
        }

        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let mut failures: Vec<(u64, Outcome)> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..threads as u64)
                .map(|thread| {
                    let seeds = seeds.clone();
                    scope.spawn(move || {
                        seeds
                            .filter(|seed| seed % threads as u64 == thread)
                            .map(|seed| (seed, self.simulate_caught(world, seed)))
                            .filter(|(_, outcome)| outcome.failure.is_some())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            runs.into_iter()
                .flat_map(|run| run.join().expect("Panics are caught for every seed"))
                .collect()
        });
        if failures.is_empty() {
            return;
        }

        failures.sort_by_key(|(seed, _)| *seed);
        let mut message = String::new();
        for (seed, outcome) in &failures {
            let failure = outcome.failure.as_ref().expect("Only failures are kept");
            message += &format!(
                "seed {seed}: {} after input {}\n",
                failure.message, failure.input
            );
        }
        let (seed, outcome) = &failures[0];
        panic!(
            "{} of the simulations of seeds {seeds:?} failed:\n{message}\n{}\nSIM_SEED={seed} runs only seed {seed}.",
            failures.len(),
            outcome.report
        );
    }

    /// Simulates the world of a seed. A panic, e.g. in the rules of a mode, becomes
    /// the failure of the seed instead of taking down every other seed.
    fn simulate_caught(&self, world: fn(&Simulator, u64) -> World, seed: u64) -> Outcome {
        let run = std::panic::AssertUnwindSafe(|| self.simulate(&world(self, seed), seed));
        std::panic::catch_unwind(run).unwrap_or_else(|panic| {
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("no message");
            Outcome {
                report: format!("PANICKED: {message}\n"),
                inputs: Vec::new(),
                failure: Some(Failure {
                    input: 0,
                    message: format!("The simulation panicked: {message}"),
                }),
            }
        })
    }

    fn run(&self, world: &World, seed: u64, header: String) -> Outcome {
        let engine = engine();
        let mut report = Report::new(header, world);
        let mut server = Server::new(world, seed);
        let mut inputs = Vec::new();
        let mut replies = Vec::new();

        let start = (self.rules)(&world.settings).and_then(|rules| {
            let driver = engine
                .instantiate(&self.component, &world.settings)
                .map_err(|e| e.to_string())?;
            Ok((Checker::new(world, rules), driver))
        });
        let (mut checker, mut driver) = match start {
            Ok(started) => started,
            Err(message) => {
                return Outcome {
                    report: format!("FAILED to start: {message}\n"),
                    inputs,
                    failure: Some(Failure { input: 0, message }),
                };
            }
        };

        let mut failure = None;
        while let Some(step) = server.next() {
            match step {
                Step::Input(input) => {
                    let index = inputs.len();
                    let reply = match driver.apply(&input) {
                        Ok(reply) => reply,
                        Err(error) => {
                            inputs.push(input);
                            failure = Some(Failure {
                                input: index,
                                message: format!("The driver failed: {error}"),
                            });
                            break;
                        }
                    };
                    server.on_reply(&input, &reply);
                    report.after(&input, &reply, &driver, &mut server);
                    let checked = checker.after(&input, &reply, &driver, &server);
                    inputs.push(input);
                    replies.push(reply);
                    if let Err(message) = checked {
                        failure = Some(Failure {
                            input: index,
                            message,
                        });
                        break;
                    }
                }
                Step::RestartBridge => {
                    match engine.replay(&self.component, &world.settings, &inputs) {
                        Ok(resumed) => driver = resumed,
                        Err(error) => {
                            failure = Some(Failure {
                                input: inputs.len(),
                                message: format!("The driver did not resume: {error}"),
                            });
                            break;
                        }
                    }
                }
            }
        }

        if failure.is_none() {
            failure = self
                .check_replays(&world.settings, seed, &inputs, &replies, &driver)
                .err();
        }
        let report = report.finish(&driver, &mut server, inputs.len(), failure.as_ref());
        Outcome {
            report,
            inputs,
            failure,
        }
    }

    /// The driver is the result of its inputs: replaying all of them gives the same
    /// driver, and a driver resumed at a random input answers the rest the same way.
    fn check_replays(
        &self,
        settings: &str,
        seed: u64,
        inputs: &[DriverInput],
        replies: &[DriverReply],
        driver: &Driver,
    ) -> Result<(), Failure> {
        let engine = engine();
        let failure = |input, message: String| Failure { input, message };
        let last = inputs.len().saturating_sub(1);

        let replayed = engine
            .replay(&self.component, settings, inputs)
            .map_err(|e| failure(last, format!("The replay failed: {e}")))?;
        if replayed.standings() != driver.standings()
            || replayed.last_round() != driver.last_round()
            || replayed.users() != driver.users()
        {
            return Err(failure(
                last,
                "Replaying every input gives another driver".into(),
            ));
        }

        let cut = Rng::new(seed).fork("resume").below(inputs.len() as u32 + 1) as usize;
        let mut resumed = engine
            .replay(&self.component, settings, &inputs[..cut])
            .map_err(|e| failure(cut, format!("Resuming failed: {e}")))?;
        for (index, (input, reply)) in inputs.iter().zip(replies).enumerate().skip(cut) {
            let again = resumed
                .apply(input)
                .map_err(|e| failure(index, format!("The resumed driver failed: {e}")))?;
            if again != *reply {
                return Err(failure(
                    index,
                    format!("Resumed after {cut} inputs, the driver answers another way"),
                ));
            }
        }
        Ok(())
    }
}

/// The seeds a check runs: only `SIM_SEED` if it is set, otherwise as many as the
/// environment variable `count` says, or as many as `default` has. They start at
/// `SIM_SEED_START`, e.g. a number that changes with every run of a CI job so it
/// explores new worlds, or at the start of `default`.
pub fn seeds(count: &str, default: Range<u64>) -> Range<u64> {
    let number = |variable: &str| {
        let value = std::env::var(variable).ok()?;
        let number = value.parse::<u64>();
        assert!(number.is_ok(), "{variable}={value} is no number");
        number.ok()
    };
    if let Some(seed) = number("SIM_SEED") {
        return seed..seed + 1;
    }
    let start = number("SIM_SEED_START").unwrap_or(default.start);
    let count = number(count).unwrap_or(default.end - default.start);
    start..start + count
}

/// One engine for every simulation of a process.
fn engine() -> &'static DriverEngine {
    static ENGINE: OnceLock<DriverEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let limits = DriverLimits {
            // Fuel limits every call the same way on every machine. A wall clock limit
            // would let a busy machine fail seeds that pass on their own.
            call_timeout: Duration::from_secs(24 * 60 * 60),
            ..DriverLimits::default()
        };
        DriverEngine::new(limits).expect("The driver engine could not start")
    })
}
