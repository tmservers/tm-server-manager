//! Builds the rounds mode the integration tests run the host with. The callbacks
//! of the server come from the simulator. The modes test their own rules with
//! simulations.
#![allow(dead_code)]

use std::{collections::BTreeMap, path::Path, sync::Mutex};

pub use tm_driver_sim::events::*;
use tm_wasm_driver::{Component, Driver, DriverEngine, DriverLimits};

pub const ROUNDS: &str = "tm-driver-mode-rounds";
pub const TIME_ATTACK: &str = "tm-driver-mode-time-attack";

/// A mode crate of the workspace as a wasm module, built once per test binary.
pub fn mode(package: &'static str) -> &'static [u8] {
    static BUILT: Mutex<BTreeMap<&str, &[u8]>> = Mutex::new(BTreeMap::new());
    // Held while building, so tests running in parallel wait for the first build.
    let mut built = BUILT.lock().unwrap();
    built.entry(package).or_insert_with(|| {
        tm_driver_sim::build_wasm(
            package,
            Path::new(env!("CARGO_MANIFEST_DIR")),
            Path::new(env!("CARGO_TARGET_TMPDIR")),
        )
        .leak()
    })
}

/// Compiles a mode crate of the workspace with the default limits.
pub fn compile(package: &'static str) -> (DriverEngine, Component) {
    let engine = DriverEngine::new(DriverLimits::default()).unwrap();
    let module = engine.compile(mode(package)).unwrap();
    (engine, module)
}

/// Starts a mode crate of the workspace with these settings.
pub fn driver(package: &'static str, settings: &str) -> Driver {
    let (engine, module) = compile(package);
    engine.instantiate(&module, settings).unwrap()
}
