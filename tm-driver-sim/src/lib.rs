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

use std::{ops::Range, sync::OnceLock};

pub use rng::Rng;
pub use rules::Rules;
use tm_wasm_driver::{Driver, DriverEngine, DriverInput, DriverLimits, DriverReply};
pub use wasm::build_wasm;
use wasmtime::component::Component;
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
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let mut failures: Vec<(u64, Outcome)> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..threads as u64)
                .map(|thread| {
                    let seeds = seeds.clone();
                    scope.spawn(move || {
                        seeds
                            .filter(|seed| seed % threads as u64 == thread)
                            .map(|seed| (seed, self.simulate(&world(self, seed), seed)))
                            .filter(|(_, outcome)| outcome.failure.is_some())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            runs.into_iter()
                .flat_map(|run| run.join().expect("A simulation panicked"))
                .collect()
        });
        if seeds.end - seeds.start == 1 && failures.is_empty() {
            println!(
                "{}",
                self.simulate(&world(self, seeds.start), seeds.start).report
            );
        }
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
            "{} of {} simulations failed:\n{message}\n{}\nSIM_SEED={seed} runs only seed {seed}.",
            failures.len(),
            seeds.end - seeds.start,
            outcome.report
        );
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

        let replayed = engine
            .replay(&self.component, settings, inputs)
            .map_err(|e| failure(inputs.len(), format!("The replay failed: {e}")))?;
        if replayed.standings() != driver.standings()
            || replayed.last_round() != driver.last_round()
            || replayed.users() != driver.users()
        {
            return Err(failure(
                inputs.len(),
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
/// environment variable `count` says from the start of `default`, or `default`.
pub fn seeds(count: &str, default: Range<u64>) -> Range<u64> {
    let number = |variable: &str| std::env::var(variable).ok()?.parse::<u64>().ok();
    match (number("SIM_SEED"), number(count)) {
        (Some(seed), _) => seed..seed + 1,
        (None, Some(count)) => default.start..default.start + count,
        (None, None) => default,
    }
}

/// One engine for every simulation of a process.
fn engine() -> &'static DriverEngine {
    static ENGINE: OnceLock<DriverEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        DriverEngine::new(DriverLimits::default()).expect("The driver engine could not start")
    })
}
