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
//! has to hold for every mode and for the mode of the world. Once the simulation
//! ended, replaying every input has to give the same driver, and a driver resumed
//! at a random input has to answer the rest the same way. The report of a
//! simulation describes the match, which the snapshot tests keep.

mod check;
mod modes;
mod report;
mod rng;
pub mod scenario;
mod server;
mod world;

pub use modes::Mode;
pub use rng::Rng;
use tm_wasm_driver::{Driver, DriverInput, DriverReply};
pub use world::{
    Action, Faults, MapSpec, Moment, PlayerSpec, RacePlan, Races, ScriptedRound, World,
};

use crate::{
    check::Checker,
    report::Report,
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

/// Simulates a random world of a mode.
pub fn simulate_seed(mode: Mode, seed: u64) -> Outcome {
    simulate(&World::random(mode, seed), seed, Some(seed))
}

/// Simulates a scenario.
pub fn simulate_scenario(world: &World) -> Outcome {
    simulate(world, 0, None)
}

fn simulate(world: &World, seed: u64, shown_seed: Option<u64>) -> Outcome {
    let (engine, component) = world.mode.compiled();
    let mut report = Report::new(world, shown_seed);
    let mut server = Server::new(world, seed);
    let mut inputs = Vec::new();
    let mut replies = Vec::new();

    let start = Checker::new(world).and_then(|checker| {
        let driver = engine
            .instantiate(&component, &world.settings)
            .map_err(|e| e.to_string())?;
        Ok((checker, driver))
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
            Step::RestartBridge => match engine.replay(&component, &world.settings, &inputs) {
                Ok(resumed) => driver = resumed,
                Err(error) => {
                    failure = Some(Failure {
                        input: inputs.len(),
                        message: format!("The driver did not resume: {error}"),
                    });
                    break;
                }
            },
        }
    }

    if failure.is_none() {
        failure = check_replays(world, seed, &inputs, &replies, &driver).err();
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
    world: &World,
    seed: u64,
    inputs: &[DriverInput],
    replies: &[DriverReply],
    driver: &Driver,
) -> Result<(), Failure> {
    let (engine, component) = world.mode.compiled();
    let failure = |input, message: String| Failure { input, message };

    let replayed = engine
        .replay(&component, &world.settings, inputs)
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
        .replay(&component, &world.settings, &inputs[..cut])
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
