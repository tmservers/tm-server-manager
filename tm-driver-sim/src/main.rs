//! Runs simulations from the command line.
//!
//! ```sh
//! cargo run -p tm-driver-sim -- knockout 42              # the report of seed 42
//! cargo run -p tm-driver-sim -- knockout 1..1000         # checks seeds, prints failures
//! cargo run -p tm-driver-sim -- path/to/match.scn        # the report of a scenario
//! cargo run -p tm-driver-sim -- knockout 42 inputs.json  # also writes the inputs
//! ```
//!
//! Sweeps are faster with `--release`.

use std::process::ExitCode;

use tm_driver_sim::{Mode, Outcome, scenario, simulate_scenario, simulate_seed};

const USAGE: &str =
    "Usage: tm-driver-sim <mode> <seed>|<first>..<last> [inputs.json] | <scenario> [inputs.json]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(passed) if passed => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// True if every simulation kept every check.
fn run(args: &[String]) -> Result<bool, String> {
    let first = args.first().ok_or("Which mode or scenario?")?;
    let Some(mode) = Mode::from_name(first) else {
        let text = std::fs::read_to_string(first).map_err(|e| format!("{first}: {e}"))?;
        let world = scenario::parse(&text)?;
        return single(simulate_scenario(&world), args.get(1));
    };

    let seeds = args.get(1).ok_or("Which seed?")?;
    match seeds.split_once("..") {
        Some((from, to)) => {
            let from: u64 = from.parse().map_err(|_| format!("{from} is no seed"))?;
            let to: u64 = to.parse().map_err(|_| format!("{to} is no seed"))?;
            let mut passed = true;
            for seed in from..to {
                if let Some(failure) = simulate_seed(mode, seed).failure {
                    passed = false;
                    println!(
                        "seed {seed}: {} after input {}",
                        failure.message, failure.input
                    );
                }
            }
            println!(
                "{} seeds of {} checked",
                to.saturating_sub(from),
                mode.name()
            );
            Ok(passed)
        }
        None => {
            let seed = seeds.parse().map_err(|_| format!("{seeds} is no seed"))?;
            single(simulate_seed(mode, seed), args.get(2))
        }
    }
}

fn single(outcome: Outcome, inputs: Option<&String>) -> Result<bool, String> {
    print!("{}", outcome.report);
    if let Some(path) = inputs {
        let json = serde_json::to_string_pretty(&outcome.inputs).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(outcome.failure.is_none())
}
