//! Runs wasm modes that drive the generic driver modescript of a Trackmania server.
//!
//! A mode is a component implementing the `mode` world of `wit/driver.wit`.
//! The host keeps it in a sandbox: no WASI, a fuel and a wall clock budget for
//! every call and a memory limit. On top of the mode it handles the transport
//! details of the modescript protocol: sessions, acknowledging decision
//! callbacks and answering repeated callbacks without stepping the mode twice.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use tm_server_types::{event::DriverEvent, method::DriverCommand};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker},
};

mod convert;

wasmtime::component::bindgen!({
    path: "wit",
    world: "mode",
});

use tmservers::driver::host::{Host, LogLevel};

/// How often the epoch of the engine advances. This is the precision of call timeouts.
const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Longest log message of a mode in bytes. Longer messages get truncated.
const MAX_LOG_MESSAGE: usize = 1024;
/// Log messages a mode may emit during a single call.
const LOG_BUDGET: u32 = 64;

/// Resources a mode may use.
#[derive(Debug, Clone, Copy)]
pub struct DriverLimits {
    /// Fuel available to every call into a mode. Roughly the number of executed instructions.
    pub fuel_per_call: u64,
    /// Wall clock time a call may take before it gets interrupted.
    pub call_timeout: Duration,
    /// Maximum linear memory of a mode in bytes.
    pub max_memory: usize,
}

impl Default for DriverLimits {
    fn default() -> Self {
        Self {
            fuel_per_call: 1_000_000_000,
            call_timeout: Duration::from_secs(1),
            max_memory: 64 << 20,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("Not a valid driver mode: {0}")]
    InvalidModule(String),
    #[error("The mode could not be instantiated: {0:#}")]
    Instantiate(wasmtime::Error),
    #[error("The mode rejected its settings or snapshot: {0}")]
    Rejected(String),
    #[error("The mode trapped: {0:#}")]
    Trap(wasmtime::Error),
    #[error("The mode trapped before and can not be used anymore.")]
    Poisoned,
}

/// Compiles and instantiates modes. Create one per process and share it.
pub struct DriverEngine {
    engine: Engine,
    linker: Linker<HostState>,
    limits: DriverLimits,
    ticker_stop: Arc<AtomicBool>,
}

impl DriverEngine {
    pub fn new(limits: DriverLimits) -> Result<Self, DriverError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).map_err(DriverError::Instantiate)?;

        let mut linker = Linker::new(&engine);
        Mode::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)
            .map_err(DriverError::Instantiate)?;

        // Advances the epoch so calls can be interrupted after their timeout.
        let ticker_stop = Arc::new(AtomicBool::new(false));
        {
            let engine = engine.clone();
            let stop = ticker_stop.clone();
            thread::Builder::new()
                .name("tm-wasm-driver-epoch".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        thread::sleep(EPOCH_TICK);
                        engine.increment_epoch();
                    }
                })
                .map_err(|e| DriverError::Instantiate(wasmtime::Error::new(e)))?;
        }

        Ok(Self {
            engine,
            linker,
            limits,
            ticker_stop,
        })
    }

    /// Compiles a mode. Accepts a component or a core module built with `tm-driver-sdk`
    /// for `wasm32-unknown-unknown`, which gets wrapped into a component.
    pub fn compile(&self, wasm: &[u8]) -> Result<DriverModule, DriverError> {
        let component = match WasmKind::of(wasm) {
            Some(WasmKind::Component) => Component::new(&self.engine, wasm),
            Some(WasmKind::Module) => {
                let encoded = wit_component::ComponentEncoder::default()
                    .module(wasm)
                    .and_then(|encoder| encoder.validate(true).encode())
                    .map_err(|e| DriverError::InvalidModule(format!("{e:#}")))?;
                Component::new(&self.engine, encoded)
            }
            None => {
                return Err(DriverError::InvalidModule(
                    "neither a wasm component nor a core module".into(),
                ));
            }
        }
        .map_err(|e| DriverError::InvalidModule(format!("{e:#}")))?;

        Ok(DriverModule { component })
    }

    /// Starts a new mode with the json settings of the match.
    pub fn instantiate(
        &self,
        module: &DriverModule,
        settings: &str,
    ) -> Result<Driver, DriverError> {
        let mut driver = self.instantiate_raw(module)?;
        driver.prepare_call();
        driver
            .bindings
            .call_init(&mut driver.store, settings)
            .map_err(DriverError::Trap)?
            .map_err(DriverError::Rejected)?;
        Ok(driver)
    }

    /// Continues a mode from a snapshot taken with [`Driver::snapshot`].
    pub fn restore(
        &self,
        module: &DriverModule,
        snapshot: DriverSnapshot,
    ) -> Result<Driver, DriverError> {
        let mut driver = self.instantiate_raw(module)?;
        driver.prepare_call();
        driver
            .bindings
            .call_restore(&mut driver.store, &snapshot.state)
            .map_err(DriverError::Trap)?
            .map_err(DriverError::Rejected)?;
        driver.session = snapshot.session;
        driver.last_decision = snapshot.last_decision;
        Ok(driver)
    }

    fn instantiate_raw(&self, module: &DriverModule) -> Result<Driver, DriverError> {
        let state = HostState {
            limits: StoreLimitsBuilder::new()
                .memory_size(self.limits.max_memory)
                .instances(16)
                .memories(4)
                .tables(16)
                .build(),
            log_budget: LOG_BUDGET,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        // Instantiation runs start functions so it gets a budget as well.
        store
            .set_fuel(self.limits.fuel_per_call)
            .map_err(DriverError::Instantiate)?;
        store.set_epoch_deadline(self.epoch_deadline());

        let bindings = Mode::instantiate(&mut store, &module.component, &self.linker)
            .map_err(DriverError::Instantiate)?;

        Ok(Driver {
            store,
            bindings,
            fuel_per_call: self.limits.fuel_per_call,
            epoch_deadline: self.epoch_deadline(),
            session: None,
            last_decision: None,
            poisoned: false,
        })
    }

    fn epoch_deadline(&self) -> u64 {
        (self.limits.call_timeout.as_millis() / EPOCH_TICK.as_millis()).max(1) as u64
    }
}

impl Drop for DriverEngine {
    fn drop(&mut self) {
        self.ticker_stop.store(true, Ordering::Relaxed);
    }
}

/// A compiled mode. Instantiating it again is cheap.
#[derive(Clone)]
pub struct DriverModule {
    component: Component,
}

/// A running mode for one match.
pub struct Driver {
    store: Store<HostState>,
    bindings: Mode,
    fuel_per_call: u64,
    epoch_deadline: u64,
    session: Option<i32>,
    /// Reply to the last decision callback in case the modescript asks again.
    last_decision: Option<(u32, Vec<DriverCommand>)>,
    poisoned: bool,
}

impl Driver {
    /// Feeds a callback of the driver modescript into the mode and returns the
    /// commands to send back in order.
    ///
    /// Decision callbacks always end with [`DriverCommand::Ack`]. A repeated
    /// decision callback gets the same reply without stepping the mode again so
    /// nothing is applied twice.
    pub fn handle(&mut self, event: &DriverEvent) -> Result<Vec<DriverCommand>, DriverError> {
        if self.poisoned {
            return Err(DriverError::Poisoned);
        }

        // A new session means the modescript restarted and counts from the start again.
        if self.session != Some(event.session()) {
            self.session = Some(event.session());
            self.last_decision = None;
        }

        if event.is_decision()
            && let Some((seq, reply)) = &self.last_decision
        {
            if *seq == event.seq() {
                return Ok(reply.clone());
            }
            if *seq > event.seq() {
                tracing::warn!(
                    "Ignoring {} with seq {} because seq {seq} was already decided.",
                    event.callback_name(),
                    event.seq()
                );
                return Ok(Vec::new());
            }
        }

        let mut commands = match convert::event(event) {
            Some(wit_event) => {
                self.prepare_call();
                match self.bindings.call_step(&mut self.store, &wit_event) {
                    Ok(commands) => commands.into_iter().map(DriverCommand::from).collect(),
                    Err(error) => {
                        self.poisoned = true;
                        return Err(DriverError::Trap(error));
                    }
                }
            }
            None => Vec::new(),
        };

        if event.is_decision() {
            commands.push(DriverCommand::Ack(event.seq()));
            self.last_decision = Some((event.seq(), commands.clone()));
        }

        Ok(commands)
    }

    /// Captures everything needed to continue the mode with [`DriverEngine::restore`].
    pub fn snapshot(&mut self) -> Result<DriverSnapshot, DriverError> {
        if self.poisoned {
            return Err(DriverError::Poisoned);
        }
        self.prepare_call();
        let state = self
            .bindings
            .call_snapshot(&mut self.store)
            .map_err(|error| {
                self.poisoned = true;
                DriverError::Trap(error)
            })?;

        Ok(DriverSnapshot {
            session: self.session,
            last_decision: self.last_decision.clone(),
            state,
        })
    }

    fn prepare_call(&mut self) {
        // Setting fuel only fails if fuel is not enabled, which the engine always does.
        _ = self.store.set_fuel(self.fuel_per_call);
        self.store.set_epoch_deadline(self.epoch_deadline);
        self.store.data_mut().log_budget = LOG_BUDGET;
    }
}

/// State of a running mode which can be persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverSnapshot {
    pub session: Option<i32>,
    pub last_decision: Option<(u32, Vec<DriverCommand>)>,
    /// Opaque state returned by the mode.
    pub state: Vec<u8>,
}

struct HostState {
    limits: StoreLimits,
    log_budget: u32,
}

// The types interface only contains types but is still an import of the world.
impl tmservers::driver::types::Host for HostState {}

impl Host for HostState {
    fn log(&mut self, level: LogLevel, mut message: String) {
        if self.log_budget == 0 {
            return;
        }
        self.log_budget -= 1;

        if message.len() > MAX_LOG_MESSAGE {
            let mut end = MAX_LOG_MESSAGE;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
        }

        match level {
            LogLevel::Trace => tracing::trace!(target: "tm_wasm_driver::mode", "{message}"),
            LogLevel::Debug => tracing::debug!(target: "tm_wasm_driver::mode", "{message}"),
            LogLevel::Info => tracing::info!(target: "tm_wasm_driver::mode", "{message}"),
            LogLevel::Warn => tracing::warn!(target: "tm_wasm_driver::mode", "{message}"),
            LogLevel::Error => tracing::error!(target: "tm_wasm_driver::mode", "{message}"),
        }
    }
}

enum WasmKind {
    Module,
    Component,
}

impl WasmKind {
    /// Reads the preamble: magic followed by the version (1 for modules, 13 for components).
    fn of(wasm: &[u8]) -> Option<Self> {
        match wasm.get(0..8)? {
            [0, b'a', b's', b'm', 1, 0, 0, 0] => Some(Self::Module),
            [0, b'a', b's', b'm', 0x0d, 0, 1, 0] => Some(Self::Component),
            _ => None,
        }
    }
}
