//! The rounds mode the integration tests run the host with. The callbacks of the
//! server come from the simulator. The modes test their own rules with
//! simulations.
#![allow(dead_code)]

use tm_mode_driver::{Driver, DriverError, DriverInput, DriverLimits};
use tm_mode_driver_sdk::Native;
pub use tm_mode_driver_sim::events::*;
use tm_mode_rounds::Rounds;

/// Starts the rounds mode with these settings, which it may reject.
pub fn start(settings: &str) -> Result<Driver, DriverError> {
    Driver::start(&Native::<Rounds>::new(), settings, DriverLimits::default())
}

/// Starts the rounds mode with these settings.
pub fn driver(settings: &str) -> Driver {
    start(settings).unwrap()
}

/// The driver of a rounds match rebuilt from its settings and inputs.
pub fn replay(settings: &str, inputs: &[DriverInput]) -> Driver {
    Driver::replay(
        &Native::<Rounds>::new(),
        settings,
        DriverLimits::default(),
        inputs,
    )
    .unwrap()
}
