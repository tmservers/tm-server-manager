//! Deterministic simulation testing for the wasm driver and its modes.
//!
//! A simulation runs the real host with a real wasm mode against a fake server.
//! The fake server plays the driver modescript and its players: it sends the
//! callbacks the driver handles, as [`DriverInput`]s, and follows the commands the
//! driver answers with, so the driver steers the match like on a real server.
//! Everything else, players, their races, time and what goes wrong, comes from a
//! random [`World`] of a seed.
//!
//! Nothing depends on the wall clock or the order threads run in, so a seed always
//! gives the same simulation. After every input, checks test what has to hold for
//! every mode, and at the end of every round and map the [`Rules`] of the mode.
//! Once the simulation ended, replaying every input has to give the same driver,
//! and a driver resumed at a random input has to answer the rest the same way. The
//! report of a simulation describes the match, e.g. for snapshots.
//!
//! The same checks run on an [`InputLog`]: inputs written directly (see
//! [`events`]), e.g. for a callback order the fake server does not produce, or the
//! inputs of a seed that failed, shrunk to what matters.
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
//! tm_driver_sim::simulation_tests!(simulator);
//! ```

mod check;
pub mod events;
mod observe;
pub mod random;
mod report;
mod rng;
pub mod rules;
mod server;
mod wasm;
mod world;

use std::{
    collections::BTreeSet,
    ops::Range,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use serde_json::Value;

pub use events::InputLog;
pub use rng::Rng;
pub use rules::Rules;
use tm_wasm_driver::{Component, Driver, DriverEngine, DriverInput, DriverLimits, DriverReply};
pub use wasm::build_wasm;
pub use world::{
    Action, Faults, MAX_PLAYERS, MapSpec, Moment, PlayerSpec, RacePlan, Races, SettingsChange,
    World,
};

use crate::{
    check::Checker,
    observe::{Observer, SeenHold},
    report::Report,
    rules::NoRules,
    server::{Server, Step},
};

/// The tests every mode runs with its simulator: random matches, full servers, the
/// matches of seeds 1 to 3 as snapshots and the logs saved in
/// `tests/simulation/logs`. Takes the function that returns the simulator.
#[macro_export]
macro_rules! simulation_tests {
    ($simulator:ident) => {
        #[test]
        fn random_matches_follow_the_rules() {
            $simulator().check(
                $crate::seeds("SIM_SEEDS", 100..125),
                $crate::Simulator::world,
            );
        }

        #[test]
        fn full_servers_follow_the_rules() {
            $simulator().check(
                $crate::seeds("SIM_FULL_SERVERS", 100..102),
                $crate::Simulator::full_server,
            );
        }

        #[test]
        fn snapshots_of_random_matches() {
            for seed in 1..=3 {
                let outcome = $simulator().seed(seed);
                assert_eq!(outcome.failure, None, "\n{}", outcome.report);
                ::insta::assert_snapshot!(format!("seed_{seed}"), outcome.report);
            }
        }

        #[test]
        fn saved_logs_pass() {
            let logs = ::std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("simulation")
                .join("logs");
            $simulator().check_logs(logs);
        }
    };
}

/// How a simulation went.
pub struct Outcome {
    /// What happened as text: one block per round with the races, the places and
    /// the standings after it.
    pub report: String,
    /// Everything the driver got, e.g. to replay it.
    pub inputs: Vec<DriverInput>,
    /// The inputs whose reply never reached the server, counted from 0.
    pub lost: Vec<usize>,
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
    /// The settings that can change during a match.
    reloadable: Vec<String>,
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
        let reloadable = engine()
            .instantiate(&component, "{}")
            .map(|driver| driver.reloadable_settings().to_vec())
            .unwrap_or_default();
        Self {
            name,
            component,
            settings: |_| "{}".into(),
            rules: |_| Ok(Box::new(NoRules)),
            reloadable,
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
        let mut world = World::random(seed, self.random_settings(seed));
        world.settings_changes = self.settings_changes(seed, &world.settings);
        world
    }

    /// The random world of a seed on a full server.
    pub fn full_server(&self, seed: u64) -> World {
        let mut world = World::full_server(seed, self.random_settings(seed));
        world.settings_changes = self.settings_changes(seed, &world.settings);
        world
    }

    fn random_settings(&self, seed: u64) -> String {
        (self.settings)(&mut Rng::new(seed).fork("settings"))
    }

    /// New settings an admin could give the match, in order: the settings that can
    /// change from other random settings, the rest as it is. Now and then one also
    /// changes a setting that cannot change, which the mode has to reject.
    fn settings_changes(&self, seed: u64, first: &str) -> Vec<SettingsChange> {
        let Ok(Value::Object(mut current)) = serde_json::from_str::<Value>(first) else {
            return Vec::new();
        };
        let rng = Rng::new(seed).fork("settings changes");
        let mut changes = Vec::new();
        for change in 0..4 {
            let random = (self.settings)(&mut rng.fork(&change.to_string()));
            let Ok(Value::Object(random)) = serde_json::from_str::<Value>(&random) else {
                break;
            };
            let mut next = current.clone();
            for name in &self.reloadable {
                match random.get(name) {
                    Some(value) => next.insert(name.clone(), value.clone()),
                    None => next.remove(name),
                };
            }
            let fixed = random.iter().find(|(name, value)| {
                !self.reloadable.contains(name)
                    && current.get(*name).is_some_and(|old| old != *value)
            });
            if let Some((name, value)) = fixed
                && rng.fork(&format!("{change}/fixed")).chance(250)
            {
                let mut rejected = next;
                rejected.insert(name.clone(), value.clone());
                changes.push(SettingsChange {
                    settings: Value::Object(rejected).to_string(),
                    reloadable: false,
                });
                continue;
            }
            current = next.clone();
            changes.push(SettingsChange {
                settings: Value::Object(next).to_string(),
                reloadable: true,
            });
        }
        changes
    }

    /// Simulates the random world of a seed.
    pub fn seed(&self, seed: u64) -> Outcome {
        self.simulate(&self.world(seed), seed)
    }

    /// Simulates a world. The seed decides what the world leaves open, e.g. the
    /// order of callbacks at the same moment.
    pub fn simulate(&self, world: &World, seed: u64) -> Outcome {
        let header = format!("{} · seed {seed}", self.name);
        let mut run = match Run::start(self, header, &world.settings, Some(world)) {
            Ok(run) => run,
            Err(outcome) => return outcome,
        };
        let mut server = Server::new(world, seed);
        let mut failure = None;
        while let Some(step) = server.next() {
            run.notes.extend(server.take_notes());
            let done = match step {
                Step::Input(input) => run.apply(&input).and_then(|(hold, reply)| {
                    let reached = server.on_reply(&input, &reply);
                    run.record(input, hold, reply, reached)
                }),
                Step::RestartBridge => run.restart(),
                Step::Crash { .. } => unreachable!("The server keeps crashes to itself"),
            };
            if let Err(error) = done {
                failure = Some(error);
                break;
            }
        }
        run.notes.extend(server.take_notes());
        if failure.is_none()
            && let Some(message) = server.failed()
        {
            failure = Some(Failure {
                input: run.inputs.len().saturating_sub(1),
                message: message.into(),
            });
        }
        run.finish(seed, server.stopped(), failure)
    }

    /// Runs a log with every check, like a simulation that does not follow the
    /// driver. The report calls it by `name`.
    pub fn log(&self, name: &str, log: &InputLog) -> Outcome {
        let header = format!("{} · {name}", self.name);
        let mut run = match Run::start(self, header, &log.settings, None) {
            Ok(run) => run,
            Err(outcome) => return outcome,
        };
        let mut failure = None;
        for (index, input) in log.inputs.iter().enumerate() {
            let reached = !log.lost.contains(&index);
            let done = run
                .apply(input)
                .and_then(|(hold, reply)| run.record(input.clone(), hold, reply, reached));
            if let Err(error) = done {
                failure = Some(error);
                break;
            }
        }
        run.finish(0, None, failure)
    }

    /// Runs every log in a directory, e.g. the shrunk logs of seeds that failed
    /// once, and panics with every failure. Nothing happens without the directory.
    pub fn check_logs(&self, dir: impl AsRef<Path>) {
        let Ok(entries) = std::fs::read_dir(dir.as_ref()) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| Some(entry.ok()?.path()))
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect();
        paths.sort();

        let mut failures = String::new();
        for path in &paths {
            let name = path
                .file_stem()
                .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
            let log = InputLog::load(path).unwrap_or_else(|error| panic!("{error}"));
            let outcome = self.log(&name, &log);
            if let Some(failure) = outcome.failure {
                failures += &format!(
                    "{name}: {} after input {}\n{}\n",
                    failure.message, failure.input, outcome.report
                );
            }
        }
        assert!(failures.is_empty(), "Saved logs failed:\n{failures}");
    }

    /// The shortest log this finds that fails with the same message as `log`, by
    /// leaving out ever smaller runs of inputs. A log that does not fail comes back
    /// as it is.
    pub fn shrink(&self, log: &InputLog) -> InputLog {
        let Some(target) = self.log("shrinking", log).failure else {
            return log.clone();
        };
        // Every input with whether its reply got lost.
        let to_log = |inputs: &[(DriverInput, bool)]| InputLog {
            settings: log.settings.clone(),
            inputs: inputs.iter().map(|(input, _)| input.clone()).collect(),
            lost: (0..inputs.len()).filter(|&index| inputs[index].1).collect(),
            ..InputLog::default()
        };
        let failing_at = |inputs: &[(DriverInput, bool)]| {
            self.log("shrinking", &to_log(inputs))
                .failure
                .filter(|failure| failure.message == target.message)
                .map(|failure| failure.input)
        };

        // Nothing after the failure matters.
        let mut inputs: Vec<(DriverInput, bool)> = log
            .inputs
            .iter()
            .enumerate()
            .take(target.input + 1)
            .map(|(index, input)| (input.clone(), log.lost.contains(&index)))
            .collect();
        let mut chunk = inputs.len().div_ceil(2).max(1);
        loop {
            let mut start = 0;
            let mut removed = false;
            while start < inputs.len() {
                let end = (start + chunk).min(inputs.len());
                let candidate: Vec<(DriverInput, bool)> = inputs[..start]
                    .iter()
                    .chain(&inputs[end..])
                    .cloned()
                    .collect();
                match failing_at(&candidate) {
                    Some(at) => {
                        inputs = candidate;
                        inputs.truncate(at + 1);
                        removed = true;
                    }
                    None => start = end,
                }
            }
            if chunk == 1 && !removed {
                break;
            }
            if !removed {
                chunk = chunk.div_ceil(2);
            }
        }
        to_log(&inputs)
    }

    /// Simulates the worlds of these seeds in parallel and panics with every failure
    /// and the report of the first one. A single seed prints its report. With
    /// `SIM_SAVE` set to a directory, the inputs of the first failure are shrunk and
    /// saved there as a log, e.g. to keep it in `tests/simulation/logs`.
    pub fn check(&self, seeds: Range<u64>, world: fn(&Simulator, u64) -> World) {
        if seeds.end - seeds.start == 1 {
            let outcome = self.simulate_caught(world, seeds.start);
            println!("{}", outcome.report);
            if let Some(failure) = &outcome.failure {
                let saved = self.save(world, seeds.start, &outcome);
                panic!(
                    "seed {}: {} after input {}{saved}",
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
        let saved = self.save(world, *seed, outcome);
        panic!(
            "{} of the simulations of seeds {seeds:?} failed:\n{message}\n{}\nSIM_SEED={seed} runs only seed {seed}.{saved}",
            failures.len(),
            outcome.report
        );
    }

    /// Saves the shrunk inputs of a failed simulation if `SIM_SAVE` names a
    /// directory, and tells where.
    fn save(&self, world: fn(&Simulator, u64) -> World, seed: u64, outcome: &Outcome) -> String {
        let Ok(dir) = std::env::var("SIM_SAVE") else {
            return String::new();
        };
        if outcome.inputs.is_empty() {
            return "\nNothing to save, the simulation failed before its first input.".into();
        }
        let log = InputLog {
            settings: world(self, seed).settings,
            inputs: outcome.inputs.clone(),
            lost: outcome.lost.clone(),
            ..InputLog::default()
        };
        let shrunk = self.shrink(&log);
        let path = Path::new(&dir).join(format!("{}-seed-{seed}.json", self.name));
        let saved = std::fs::create_dir_all(&dir)
            .map_err(|e| format!("{dir} could not be created: {e}"))
            .and_then(|()| shrunk.save(&path));
        match saved {
            Ok(()) => format!(
                "\nThe inputs of seed {seed}, shrunk from {} to {}, are in {}.",
                log.inputs.len(),
                shrunk.inputs.len(),
                path.display()
            ),
            Err(error) => format!("\n{error}"),
        }
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
                lost: Vec::new(),
                failure: Some(Failure {
                    input: 0,
                    message: format!("The simulation panicked: {message}"),
                }),
            }
        })
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
                    "A resumed driver answers another way".into(),
                ));
            }
        }
        Ok(())
    }
}

/// A driver going through inputs while every check watches.
struct Run<'a> {
    simulator: &'a Simulator,
    settings: String,
    driver: Driver,
    observer: Observer,
    checker: Checker,
    report: Report,
    inputs: Vec<DriverInput>,
    replies: Vec<DriverReply>,
    /// The inputs whose reply never reached the server.
    lost: Vec<usize>,
    /// What happened around the inputs, for the report.
    notes: Vec<String>,
}

impl<'a> Run<'a> {
    fn start(
        simulator: &'a Simulator,
        header: String,
        settings: &str,
        world: Option<&World>,
    ) -> Result<Self, Outcome> {
        let started = (simulator.rules)(settings).and_then(|rules| {
            let driver = engine()
                .instantiate(&simulator.component, settings)
                .map_err(|e| e.to_string())?;
            Ok((rules, driver))
        });
        let (rules, driver) = started.map_err(|message| Outcome {
            report: format!("FAILED to start: {message}\n"),
            inputs: Vec::new(),
            lost: Vec::new(),
            failure: Some(Failure { input: 0, message }),
        })?;
        let maps = world.map(|world| {
            world
                .maps
                .iter()
                .map(|map| map.uid.clone())
                .collect::<BTreeSet<_>>()
        });
        Ok(Self {
            simulator,
            settings: settings.into(),
            driver,
            observer: Observer::new(settings),
            checker: Checker::new(rules, maps),
            report: Report::new(header, settings, world),
            inputs: Vec::new(),
            replies: Vec::new(),
            lost: Vec::new(),
            notes: Vec::new(),
        })
    }

    /// Hands an input to the driver. Fails for an input no server could have sent
    /// and when the driver fails.
    fn apply(&mut self, input: &DriverInput) -> Result<(Option<SeenHold>, DriverReply), Failure> {
        let index = self.inputs.len();
        let fail = |message: String| Failure {
            input: index,
            message,
        };
        let hold = self.observer.input(input).map_err(|message| {
            self.inputs.push(input.clone());
            fail(message)
        })?;
        let reply = self.driver.apply(input).map_err(|error| {
            self.inputs.push(input.clone());
            fail(format!("The driver failed: {error}"))
        })?;
        Ok((hold, reply))
    }

    /// Checks the reply to an input, which did or did not reach the server.
    fn record(
        &mut self,
        input: DriverInput,
        hold: Option<SeenHold>,
        reply: DriverReply,
        reached: bool,
    ) -> Result<(), Failure> {
        let index = self.inputs.len();
        let checked = self.observer.reply(&input, &reply, reached).and_then(|()| {
            self.notes.extend(self.observer.take_notes());
            self.report.after(
                &input,
                hold.as_ref(),
                &reply,
                &self.driver,
                &self.observer,
                &mut self.notes,
            );
            self.checker.after(
                &input,
                hold.as_ref(),
                &reply,
                reached,
                &self.driver,
                &self.observer,
            )
        });
        if !reached {
            self.lost.push(index);
        }
        self.inputs.push(input);
        self.replies.push(reply);
        checked.map_err(|message| Failure {
            input: index,
            message,
        })
    }

    /// The bridge restarts: the driver resumes by replaying its inputs.
    fn restart(&mut self) -> Result<(), Failure> {
        self.driver = engine()
            .replay(&self.simulator.component, &self.settings, &self.inputs)
            .map_err(|error| Failure {
                input: self.inputs.len().saturating_sub(1),
                message: format!("The driver did not resume: {error}"),
            })?;
        Ok(())
    }

    fn finish(mut self, seed: u64, stopped: Option<&str>, mut failure: Option<Failure>) -> Outcome {
        if failure.is_none() {
            failure = self
                .simulator
                .check_replays(
                    &self.settings,
                    seed,
                    &self.inputs,
                    &self.replies,
                    &self.driver,
                )
                .err();
        }
        let report = self.report.finish(
            &self.driver,
            &mut self.notes,
            stopped,
            self.inputs.len(),
            failure.as_ref(),
        );
        Outcome {
            report,
            inputs: self.inputs,
            lost: self.lost,
            failure,
        }
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
