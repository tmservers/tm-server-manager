//! Runs wasm modes that drive the generic driver modescript of a Trackmania server.
//!
//! A mode is a component implementing the `mode` world of `wit/driver.wit`.
//! The host keeps it in a sandbox: no WASI, a fuel and a wall clock budget for
//! every call and a memory limit.
//!
//! Modes are fed from the standard callbacks of the server. On top of that the
//! host handles the [`DriverHold`]s of the modescript: it steps the mode, lets
//! the hold continue and answers repeated holds without stepping the mode twice.
//!
//! Modes and the host are deterministic. Replaying the events of a match with
//! [`DriverEngine::replay`] rebuilds exactly the same driver, so a crashed bridge
//! resumes from the event stream and nothing else has to be persisted.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use tm_server_types::{
    event::{DriverHold, DriverRoundResult, DriverSection, Event},
    method::DriverCommand,
};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker},
};

mod convert;

// In its own module because the world defines its own `Event`.
mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "mode",
    });
}

use bindings::{
    Mode,
    tmservers::driver::{
        host::{Host, LogLevel},
        types as wit,
    },
};

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
    #[error("The mode rejected its settings: {0}")]
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
        // Replays must produce the same results on every host.
        config.cranelift_nan_canonicalization(true);
        config.relaxed_simd_deterministic(true);
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

    /// Rebuilds a driver by feeding it the events it handled before, e.g. after the
    /// bridge restarted. The replies are dropped because the server already got them.
    /// A hold the modescript repeats afterwards gets the reply it got the first time.
    pub fn replay<'a>(
        &self,
        module: &DriverModule,
        settings: &str,
        events: impl IntoIterator<Item = &'a Event>,
    ) -> Result<Driver, DriverError> {
        let mut driver = self.instantiate(module, settings)?;
        for event in events {
            driver.handle(event)?;
        }
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
            last_hold: None,
            round_results: None,
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
    /// Commands for the last hold in case the modescript asks again.
    last_hold: Option<(u32, Vec<DriverCommand>)>,
    /// Race times of the scores sent right before the round end hold.
    round_results: Option<Vec<wit::RaceResult>>,
    poisoned: bool,
}

/// What should happen after an event.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DriverReply {
    /// Commands for the server in order.
    pub commands: Vec<DriverCommand>,
    /// Results of the current round for the manager.
    pub round_result: Option<DriverRoundResult>,
}

impl Driver {
    /// Whether an event can produce a reply. Lets callers skip the rest cheaply.
    pub fn handles(event: &Event) -> bool {
        matches!(
            event,
            Event::StartMatchStart(_)
                | Event::StartRoundStart(_)
                | Event::Scores(_)
                | Event::EndMatchStart(_)
                | Event::DriverHold(_)
        )
    }

    /// Feeds a callback of the server into the mode.
    ///
    /// Holds always end with [`DriverCommand::Continue`]. A repeated hold gets the
    /// same commands without stepping the mode again so nothing is decided twice.
    pub fn handle(&mut self, event: &Event) -> Result<DriverReply, DriverError> {
        if self.poisoned {
            return Err(DriverError::Poisoned);
        }

        match event {
            Event::StartMatchStart(_) => self.step(wit::Event::MatchStart),
            Event::StartRoundStart(_) => self.step(wit::Event::RoundStart),
            Event::EndMatchStart(_) => self.step(wit::Event::MatchEnd),
            Event::Scores(scores) if scores.section == "PreEndRound" => {
                self.round_results = Some(convert::race_results(scores));
                Ok(DriverReply::default())
            }
            Event::DriverHold(hold) => self.hold(hold),
            _ => Ok(DriverReply::default()),
        }
    }

    fn hold(&mut self, hold: &DriverHold) -> Result<DriverReply, DriverError> {
        // A new session means the modescript restarted and counts from the start again.
        if self.session != Some(hold.session) {
            self.session = Some(hold.session);
            self.last_hold = None;
        }

        if let Some((seq, commands)) = &self.last_hold {
            if *seq == hold.seq {
                // The round result already went out with the first reply.
                return Ok(DriverReply {
                    commands: commands.clone(),
                    round_result: None,
                });
            }
            if *seq > hold.seq {
                tracing::warn!(
                    "Ignoring the hold with seq {} because seq {seq} already continued.",
                    hold.seq
                );
                return Ok(DriverReply::default());
            }
        }

        let event = match hold.section {
            DriverSection::MapStart => wit::Event::MapStart,
            DriverSection::RoundEnd => wit::Event::RoundEnd(wit::RoundEnd {
                forced: hold.forced,
                results: self.round_results.take().unwrap_or_default(),
            }),
            DriverSection::MapEnd => wit::Event::MapEnd,
        };

        let mut reply = self.step(event)?;
        reply.commands.push(DriverCommand::Continue(hold.seq));
        self.last_hold = Some((hold.seq, reply.commands.clone()));
        Ok(reply)
    }

    fn step(&mut self, event: wit::Event) -> Result<DriverReply, DriverError> {
        self.prepare_call();
        match self.bindings.call_step(&mut self.store, &event) {
            Ok(commands) => Ok(convert::reply(commands)),
            Err(error) => {
                self.poisoned = true;
                Err(DriverError::Trap(error))
            }
        }
    }

    fn prepare_call(&mut self) {
        // Setting fuel only fails if fuel is not enabled, which the engine always does.
        _ = self.store.set_fuel(self.fuel_per_call);
        self.store.set_epoch_deadline(self.epoch_deadline);
        self.store.data_mut().log_budget = LOG_BUDGET;
    }
}

struct HostState {
    limits: StoreLimits,
    log_budget: u32,
}

// The types interface only contains types but is still an import of the world.
impl wit::Host for HostState {}

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
