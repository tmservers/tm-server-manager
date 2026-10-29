//! Runs wasm modes that drive the generic driver modescript of a Trackmania server.
//!
//! A mode is a component implementing the `mode` world of `wit/driver.wit`.
//! The host keeps it in a sandbox: no WASI, a fuel and a wall clock budget for
//! every call and a memory limit.
//!
//! Modes are fed from the standard callbacks of the server: the host collects the
//! races of a round from the start lines and waypoints and keeps the players it
//! knows. On top of that the host handles the [`DriverHold`]s of the modescript:
//! it steps the mode, lets the hold continue and answers repeated holds without
//! stepping the mode twice.
//!
//! The scores of a match live in standings every mode shares. The host keeps
//! them and hands them to the mode with every event, so modes keep nothing but
//! their settings. The host clears what belongs to a new match or map, publishes
//! the standings, the round and the players for the manialinks whenever they
//! change and reports every round the mode placed to the manager. While a round
//! runs, the mode places it as it stands whenever the races changed, at most once
//! per [`DriverLimits::live_round_interval`], so the round shows live.
//!
//! Modes and the host are deterministic. Everything a driver gets is a
//! [`DriverInput`], and replaying the inputs of a match with
//! [`DriverEngine::replay`] rebuilds exactly the same driver, so a crashed bridge
//! resumes from them and nothing else has to be persisted.
//!
//! Settings can change during a match ([`DriverInput::Settings`]). The mode keeps
//! its standings and takes the new settings from the next round on.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use tm_server_types::{
    base::{Map, PlayerInfo},
    event::{DriverHold, DriverRoundResult, DriverSection, Event},
    method::{DriverCanSpawn, DriverCommand, DriverRound, DriverStandings, DriverUsers},
};
/// A compiled mode, see [`DriverEngine::compile`].
pub use wasmtime::component::Component;
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{HasSelf, Linker},
};

mod convert;
mod races;

// In its own module because the world defines its own `Event`.
mod bindings {
    wasmtime::component::bindgen!({
        path: "wit",
        world: "mode",
    });
}

use races::Races;

use bindings::{
    Mode,
    tmservers::driver::{
        host::{Host, LogLevel},
        types as wit,
    },
};

/// How often the epoch of the engine advances. This is the precision of call timeouts.
const EPOCH_TICK: Duration = Duration::from_millis(10);
/// Log messages a mode may write in one call. Further messages are dropped.
const LOG_BUDGET: u32 = 64;
/// Longest log message of a mode in bytes. Longer messages get truncated.
const MAX_LOG_MESSAGE: usize = 1024;

/// Resources a mode may use.
#[derive(Debug, Clone, Copy)]
pub struct DriverLimits {
    /// Fuel available to every call into a mode. Roughly the number of executed instructions.
    pub fuel_per_call: u64,
    /// Wall clock time a call may take before it gets interrupted.
    pub call_timeout: Duration,
    /// Maximum linear memory of a mode in bytes.
    pub max_memory: usize,
    /// The shortest time between two live rounds, in the server time of the
    /// callbacks. What changes in between shows with the first start or checkpoint
    /// after it, and the round end always shows the final round.
    pub live_round_interval: Duration,
}

impl Default for DriverLimits {
    fn default() -> Self {
        Self {
            fuel_per_call: 1_000_000_000,
            call_timeout: Duration::from_secs(1),
            max_memory: 64 << 20,
            live_round_interval: Duration::from_secs(1),
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
        config.strategy(wasmtime::Strategy::Cranelift);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        config.epoch_interruption(true);
        // The same events have to give the same results, e.g. when a match is replayed.
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

    /// Compiles a mode. Takes a component or a core module, e.g. a mode crate built
    /// for `wasm32-unknown-unknown`, which is turned into a component.
    pub fn compile(&self, wasm: &[u8]) -> Result<Component, DriverError> {
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

        Ok(component)
    }

    /// Starts a mode for a match with its json settings, which the mode may reject.
    pub fn instantiate(
        &self,
        component: &Component,
        settings: &str,
    ) -> Result<Driver, DriverError> {
        let mut driver = self.instantiate_raw(component)?;
        driver.prepare_call();
        driver
            .bindings
            .call_init(&mut driver.store, settings)
            .map_err(DriverError::Trap)?
            .map_err(DriverError::Rejected)?;
        driver.prepare_call();
        driver.name = driver
            .bindings
            .call_name(&mut driver.store)
            .map_err(DriverError::Trap)?;
        driver.prepare_call();
        driver.reloadable = driver
            .bindings
            .call_reloadable_settings(&mut driver.store)
            .map_err(DriverError::Trap)?;
        driver.settings = settings.into();
        Ok(driver)
    }

    /// Rebuilds the driver of a match from its settings and every input it got, in
    /// order, e.g. after the bridge restarted. Needs the mode and the limits the
    /// match ran with.
    pub fn replay<'a>(
        &self,
        component: &Component,
        settings: &str,
        inputs: impl IntoIterator<Item = &'a DriverInput>,
    ) -> Result<Driver, DriverError> {
        let mut driver = self.instantiate(component, settings)?;
        for input in inputs {
            driver.apply(input)?;
        }
        Ok(driver)
    }

    fn instantiate_raw(&self, component: &Component) -> Result<Driver, DriverError> {
        let state = HostState {
            limits: StoreLimitsBuilder::new()
                .memory_size(self.limits.max_memory)
                .instances(16)
                .memories(4)
                .tables(16)
                .build(),
            log_budget: LOG_BUDGET,
            users: BTreeMap::new(),
            map_count: 0,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        // Instantiation runs start functions so it gets a budget as well.
        store
            .set_fuel(self.limits.fuel_per_call)
            .map_err(DriverError::Instantiate)?;
        store.set_epoch_deadline(self.epoch_deadline());

        let bindings = Mode::instantiate(&mut store, component, &self.linker)
            .map_err(DriverError::Instantiate)?;

        Ok(Driver {
            store,
            bindings,
            fuel_per_call: self.limits.fuel_per_call,
            epoch_deadline: self.epoch_deadline(),
            live_round_interval: u32::try_from(self.limits.live_round_interval.as_millis())
                .unwrap_or(u32::MAX),
            name: String::new(),
            settings: String::new(),
            pending_settings: None,
            reloadable: Vec::new(),
            standings: convert::empty_standings(),
            last_round: Vec::new(),
            live_round: None,
            live_round_asked: None,
            published: Published::default(),
            spawn_permissions: BTreeMap::new(),
            session: None,
            last_hold: None,
            playlist: Vec::new(),
            map: None,
            next_map_chosen: false,
            connected: BTreeSet::new(),
            in_round: false,
            in_warm_up: false,
            paused: false,
            races: Races::default(),
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

/// A running mode for one match.
pub struct Driver {
    store: Store<HostState>,
    bindings: Mode,
    fuel_per_call: u64,
    epoch_deadline: u64,
    /// See [`DriverLimits::live_round_interval`], in milliseconds.
    live_round_interval: u32,
    /// Name of the mode, shown above the standings.
    name: String,
    /// The json settings the mode plays with.
    settings: String,
    /// Settings that wait for the round that runs to end.
    pending_settings: Option<String>,
    /// The settings that can change during a match.
    reloadable: Vec<String>,
    /// The scores of the match, handed to the mode with every event.
    standings: wit::Standings,
    /// The last round that counted, as the mode placed it.
    last_round: Vec<wit::Entry>,
    /// The round that runs as the mode placed it last, None between rounds.
    live_round: Option<Vec<wit::Entry>>,
    /// When the mode placed the round that runs last, in server time.
    live_round_asked: Option<u32>,
    published: Published,
    /// Spawn permissions sent in this match, so repeated ones are left out.
    spawn_permissions: BTreeMap<String, bool>,
    session: Option<i32>,
    /// The last hold, in case the modescript asks again.
    last_hold: Option<LastHold>,
    /// Uids of the maps the match plays in order.
    playlist: Vec<String>,
    map: Option<Map>,
    /// The mode chose the next map since the current one started.
    next_map_chosen: bool,
    /// Players on the server, racing or not.
    connected: BTreeSet<String>,
    /// A round runs, from its start to its end hold. Warmup races are outside.
    in_round: bool,
    /// The built-in warmup runs.
    in_warm_up: bool,
    /// The modescript is paused.
    paused: bool,
    /// The races of the current round.
    races: Races,
    poisoned: bool,
}

/// The reply to the last hold.
struct LastHold {
    seq: u32,
    commands: Vec<DriverCommand>,
    /// The reply reached the server, see [`DriverInput::Delivered`].
    delivered: bool,
}

/// What the manialinks got last, so only changes are sent.
#[derive(Default)]
struct Published {
    users: Option<DriverUsers>,
    standings: Option<DriverStandings>,
    round: Option<DriverRound>,
}

/// What a driver gets: the callbacks of the server and what its caller knows
/// besides them. A driver is the result of its settings and its inputs in order,
/// so persisting every input is enough to resume a match by replaying them.
// Nearly every input is an event, boxing them would only add allocations.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DriverInput {
    /// A callback of the server. Only the ones [`Driver::handles`] matter.
    Event(Event),
    /// The uids of the maps the match plays from now on, e.g. the map pool of the
    /// config. The maps have to be in the map list of the server.
    Playlist(Vec<String>),
    /// The players on the server, e.g. when the driver starts or resumes during a
    /// match, since they do not connect again. The same as their `PlayerInfoChanged`
    /// callbacks, but publishes once for all of them. Players the driver thought on
    /// the server but missing here left, e.g. while the bridge was down.
    Players(Vec<PlayerInfo>),
    /// The reply to the last hold reached the server. When the modescript asks
    /// again, e.g. because the reply came late, the hold only gets its `Continue`,
    /// so commands that are not idempotent do not run twice: `EndMap` is the
    /// `NextMap` of the server, `EndMatch` could end the next match, chat messages
    /// would show twice. Without it, e.g. when the reply got lost with the bridge,
    /// the hold gets every command again. Like the reply to every input, its reply
    /// can have commands, e.g. of settings that waited for the round to end.
    Delivered,
    /// The driver resumed, e.g. the bridge after a restart, and replies to the
    /// last inputs may have been lost. Everything the driver set on the server goes
    /// out again: the spawn permissions of the match and what the manialinks show.
    Resumed,
    /// New json settings for the match, e.g. another points repartition. The mode
    /// keeps its standings and takes them from the next round on: right away when
    /// no round runs, otherwise with the first input after the round ended. A mode
    /// rejects settings it cannot change during a match
    /// ([`Driver::reloadable_settings`]), which then change nothing
    /// ([`DriverReply::rejected`]). Later settings replace ones that still wait.
    Settings(String),
}

/// What should happen after an input.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DriverReply {
    /// Commands for the server in order.
    pub commands: Vec<DriverCommand>,
    /// Results of the current round for the manager.
    pub round_result: Option<DriverRoundResult>,
    /// Why the mode rejected new settings ([`DriverInput::Settings`]), which then
    /// change nothing.
    pub rejected: Option<String>,
}

impl Driver {
    /// Whether an event can produce a reply. Lets callers skip the rest cheaply.
    pub fn handles(event: &Event) -> bool {
        matches!(
            event,
            Event::StartMatchStart(_)
                | Event::StartMapStart(_)
                | Event::WarmupStart
                | Event::WarmupEnd
                | Event::StartRoundStart(_)
                | Event::StartLine(_)
                | Event::WayPoint(_)
                | Event::PlayerConnect(_)
                | Event::PlayerDisconnect(_)
                | Event::PlayerInfoChanged(_)
                | Event::EndMatchStart(_)
                | Event::DriverHold(_)
                | Event::Pause(_)
        )
    }

    /// Applies an input. Persist every input in order to resume the match by replay.
    pub fn apply(&mut self, input: &DriverInput) -> Result<DriverReply, DriverError> {
        if let DriverInput::Event(event) = input {
            return self.handle(event);
        }
        let settings = self.due_settings()?;
        let reply = match input {
            DriverInput::Event(_) => unreachable!("Events are handled above"),
            DriverInput::Playlist(maps) => {
                self.playlist.clone_from(maps);
                // The mode reads it through the host functions.
                self.store.data_mut().map_count = self.playlist.len() as u32;
                self.published()
            }
            DriverInput::Players(players) => {
                self.connected.clear();
                for player in players {
                    self.learn(player);
                }
                self.published()
            }
            DriverInput::Resumed => {
                let mut commands: Vec<DriverCommand> = self
                    .spawn_permissions
                    .iter()
                    .map(|(account_id, &can_spawn)| {
                        DriverCommand::SetCanSpawn(DriverCanSpawn {
                            account_id: account_id.clone(),
                            can_spawn,
                        })
                    })
                    .collect();
                self.published = Published::default();
                commands.extend(self.publish());
                DriverReply {
                    commands,
                    ..DriverReply::default()
                }
            }
            DriverInput::Delivered => {
                if let Some(last_hold) = &mut self.last_hold {
                    last_hold.delivered = true;
                }
                DriverReply::default()
            }
            DriverInput::Settings(new) => self.new_settings(new)?,
        };
        Ok(after_settings(settings, reply))
    }

    /// The json settings the mode plays with, the last ones it took.
    pub fn settings(&self) -> &str {
        &self.settings
    }

    /// Settings that wait for the round that runs to end.
    pub fn pending_settings(&self) -> Option<&str> {
        self.pending_settings.as_deref()
    }

    /// The settings that can change during a match, by their name in the json.
    pub fn reloadable_settings(&self) -> &[String] {
        &self.reloadable
    }

    /// Takes new settings once no round runs. The mode checks them right away, so
    /// the caller learns at once when it rejects them.
    fn new_settings(&mut self, settings: &str) -> Result<DriverReply, DriverError> {
        if self.poisoned {
            return Err(DriverError::Poisoned);
        }
        if !self.in_round {
            return self.change_settings(settings);
        }
        // The round that runs ends with the settings it started with.
        self.prepare_call();
        let checked = self
            .bindings
            .call_check_settings(&mut self.store, settings)
            .map_err(|error| {
                self.poisoned = true;
                DriverError::Trap(error)
            })?;
        match checked {
            Ok(()) => {
                self.pending_settings = Some(settings.into());
                Ok(DriverReply::default())
            }
            Err(reason) => Ok(DriverReply {
                rejected: Some(reason),
                ..DriverReply::default()
            }),
        }
    }

    /// Gives the mode new settings and lets it react, e.g. by sending the round
    /// format again.
    fn change_settings(&mut self, settings: &str) -> Result<DriverReply, DriverError> {
        self.prepare_call();
        let changed = self
            .bindings
            .call_change_settings(&mut self.store, settings)
            .map_err(|error| {
                self.poisoned = true;
                DriverError::Trap(error)
            })?;
        if let Err(reason) = changed {
            return Ok(DriverReply {
                rejected: Some(reason),
                ..DriverReply::default()
            });
        }
        self.settings = settings.into();
        self.step(wit::Event::SettingsChanged)
    }

    /// Settings that waited for the round to end apply with the first input after
    /// it, before the mode gets the input. So a round end and new settings never
    /// come with the same input.
    fn due_settings(&mut self) -> Result<Vec<DriverCommand>, DriverError> {
        if self.in_round {
            return Ok(Vec::new());
        }
        let Some(settings) = self.pending_settings.take() else {
            return Ok(Vec::new());
        };
        if self.poisoned {
            return Err(DriverError::Poisoned);
        }
        let reply = self.change_settings(&settings)?;
        if let Some(reason) = reply.rejected {
            tracing::warn!("The mode rejected settings it accepted during the round: {reason}");
        }
        Ok(reply.commands)
    }

    /// Feeds a callback of the server into the mode, the same as [`Driver::apply`]
    /// with [`DriverInput::Event`].
    ///
    /// Holds always end with [`DriverCommand::Continue`]. A repeated hold gets the
    /// decisions of the first reply again without stepping the mode, so nothing is
    /// decided twice, and what the manialinks show as it is now. Once the first reply
    /// was [`DriverInput::Delivered`], it only gets its `Continue`.
    pub fn handle(&mut self, event: &Event) -> Result<DriverReply, DriverError> {
        if self.poisoned {
            return Err(DriverError::Poisoned);
        }
        let settings = self.due_settings()?;
        let reply = self.event(event)?;
        Ok(after_settings(settings, reply))
    }

    fn event(&mut self, event: &Event) -> Result<DriverReply, DriverError> {
        match event {
            Event::StartMatchStart(_) => {
                // Every match starts with the defaults of the modescript.
                self.spawn_permissions.clear();
                self.last_round.clear();
                self.live_round = None;
                // Everything is published again, e.g. for a modescript that just started.
                self.published = Published::default();
                self.step(wit::Event::MatchStart)
            }
            Event::StartMapStart(start) => {
                self.map = Some(start.map.clone());
                self.next_map_chosen = false;
                // The standings show the name of the map.
                Ok(self.published())
            }
            Event::WarmupStart => {
                self.in_warm_up = true;
                Ok(DriverReply::default())
            }
            Event::WarmupEnd => {
                self.in_warm_up = false;
                Ok(DriverReply::default())
            }
            Event::StartRoundStart(_) => {
                self.races.clear();
                self.in_round = true;
                // The round shows live from its start on, empty until somebody races.
                self.live_round = (!self.in_warm_up).then(Vec::new);
                self.live_round_asked = None;
                self.step(wit::Event::RoundStart)
            }
            Event::StartLine(start_line) => {
                let first = self.races.start(&start_line.account_id);
                if !self.in_round || self.in_warm_up {
                    return Ok(DriverReply::default());
                }
                // Time attack rounds start players again and again.
                let mut reply = if first {
                    self.step(wit::Event::RaceStart(start_line.account_id.clone()))?
                } else {
                    DriverReply::default()
                };
                reply.commands.extend(self.live(start_line.time)?);
                Ok(reply)
            }
            Event::WayPoint(way_point) => {
                self.races.way_point(way_point);
                if !self.in_round || self.in_warm_up {
                    return Ok(DriverReply::default());
                }
                Ok(DriverReply {
                    commands: self.live(way_point.time)?,
                    ..DriverReply::default()
                })
            }
            Event::PlayerConnect(connect) => {
                self.connected.insert(connect.account_id.clone());
                Ok(DriverReply::default())
            }
            Event::PlayerDisconnect(disconnect) => {
                // The player stays known, e.g. for the name in the standings.
                self.connected.remove(&disconnect.account_id);
                Ok(DriverReply::default())
            }
            Event::PlayerInfoChanged(info) => {
                if !self.learn(info) {
                    return Ok(DriverReply::default());
                }
                Ok(self.published())
            }
            Event::EndMatchStart(_) => self.step(wit::Event::MatchEnd),
            Event::DriverHold(hold) => self.hold(hold),
            // Only shown. The modescript ends a round a pause interrupts as skipped, so
            // the mode gets it as invalid.
            Event::Pause(pause) => {
                self.paused = pause.available && pause.active;
                Ok(self.published())
            }
            _ => Ok(DriverReply::default()),
        }
    }

    fn hold(&mut self, hold: &DriverHold) -> Result<DriverReply, DriverError> {
        // A new session means the modescript restarted and counts from the start again.
        if self.session != Some(hold.session) {
            self.session = Some(hold.session);
            self.last_hold = None;
            self.spawn_permissions.clear();
            self.paused = false;
        }

        if let Some(last_hold) = &self.last_hold {
            if last_hold.seq == hold.seq {
                // The round result already went out with the first reply.
                let mut commands = if last_hold.delivered {
                    Vec::new()
                } else {
                    // The first reply might not have arrived. Its decisions go out again,
                    // what the manialinks show as it is now, which can be newer.
                    let mut decisions: Vec<DriverCommand> = last_hold
                        .commands
                        .iter()
                        .filter(|command| {
                            !is_published(command) && !matches!(command, DriverCommand::Continue(_))
                        })
                        .cloned()
                        .collect();
                    self.published = Published::default();
                    decisions.extend(self.publish());
                    decisions
                };
                commands.push(DriverCommand::Continue(hold.seq));
                return Ok(DriverReply {
                    commands,
                    ..DriverReply::default()
                });
            }
            if last_hold.seq > hold.seq {
                tracing::warn!(
                    "Ignoring the hold with seq {} because seq {} already continued.",
                    hold.seq,
                    last_hold.seq
                );
                return Ok(DriverReply::default());
            }
        }

        // The script waits between rounds. A round end without its start, e.g. after the
        // bridge restarted, missed races, so it is invalid like a round the script
        // forced to end or skipped.
        let in_round = std::mem::take(&mut self.in_round);
        self.live_round = None;
        let event = match hold.section {
            DriverSection::MapStart => wit::Event::MapStart,
            DriverSection::RoundEnd => wit::Event::RoundEnd(wit::RoundEnd {
                invalid: hold.forced || !in_round,
                racers: self.races.racers(),
                present: self.connected.iter().cloned().collect(),
            }),
            DriverSection::MapEnd => wit::Event::MapEnd,
        };

        let mut reply = self.step(event)?;
        reply.commands.push(DriverCommand::Continue(hold.seq));
        self.last_hold = Some(LastHold {
            seq: hold.seq,
            commands: reply.commands.clone(),
            delivered: false,
        });
        Ok(reply)
    }

    /// The standings of the match as the mode left them, ordered by position and
    /// players without one by name.
    pub fn standings(&self) -> DriverStandings {
        let map_name = self.map.as_ref().map_or("", |map| map.name.as_str());
        convert::standings(
            &self.name,
            map_name,
            self.playlist.len() as u32,
            self.paused,
            &self.standings,
            &self.store.data().users,
        )
    }

    /// The round that runs as it stands, otherwise the last round that counted,
    /// ordered by place.
    pub fn round(&self) -> DriverRound {
        match &self.live_round {
            Some(entries) => convert::round(
                self.standings.rounds_played.saturating_add(1),
                true,
                entries,
            ),
            None => convert::round(self.standings.rounds_played, false, &self.last_round),
        }
    }

    /// The players the host knows, e.g. for their names.
    pub fn users(&self) -> DriverUsers {
        convert::users(&self.store.data().users)
    }

    /// Remembers a player on the server, e.g. for the names in the standings. False
    /// for the server, which shows up as a player too.
    fn learn(&mut self, info: &PlayerInfo) -> bool {
        if info.is_server() {
            return false;
        }
        self.connected.insert(info.account_id.clone());
        let user = self
            .store
            .data_mut()
            .users
            .entry(info.account_id.clone())
            .or_insert_with(|| wit::User {
                account_id: info.account_id.clone(),
                name: String::new(),
                club_tag: String::new(),
                zone: String::new(),
            });
        user.name = info.nick_name.clone();
        true
    }

    fn step(&mut self, event: wit::Event) -> Result<DriverReply, DriverError> {
        clear(&mut self.standings, &event);
        let decision = self.call_step(&event)?;
        self.standings = decision.standings;

        let round_result = self.take_round(&event);
        let mut commands = self.commands(decision.commands);
        commands.extend(self.default_next_map(&event, &commands));
        commands.extend(self.publish());

        Ok(DriverReply {
            commands,
            round_result,
            rejected: None,
        })
    }

    /// Lets the mode place the round that runs as it stands after its races changed,
    /// at most once per interval of server time, and publishes it. Changes in between
    /// show with the next change after the interval.
    fn live(&mut self, time: u32) -> Result<Vec<DriverCommand>, DriverError> {
        if self.live_round.is_none() {
            return Ok(Vec::new());
        }
        if self
            .live_round_asked
            .is_some_and(|asked| time.wrapping_sub(asked) < self.live_round_interval)
        {
            return Ok(Vec::new());
        }
        let races = wit::RoundEnd {
            invalid: false,
            racers: self.races.racers(),
            present: self.connected.iter().cloned().collect(),
        };
        self.prepare_call();
        let round = self
            .bindings
            .call_live_round(&mut self.store, &self.standings, &races)
            .map_err(|error| {
                self.poisoned = true;
                DriverError::Trap(error)
            })?;
        self.live_round = Some(round);
        self.live_round_asked = Some(time);
        Ok(self.publish())
    }

    /// A trap poisons the driver, the mode could be in any state afterwards.
    fn call_step(&mut self, event: &wit::Event) -> Result<wit::Decision, DriverError> {
        self.prepare_call();
        self.bindings
            .call_step(&mut self.store, &self.standings, event)
            .map_err(|error| {
                self.poisoned = true;
                DriverError::Trap(error)
            })
    }

    /// Takes the round the mode placed out of the standings, the mode never gets it
    /// back. Unless the round is invalid, it becomes the last round and goes to the
    /// manager.
    fn take_round(&mut self, event: &wit::Event) -> Option<DriverRoundResult> {
        let round = std::mem::take(&mut self.standings.round);
        match event {
            wit::Event::RoundEnd(round_end) if !round_end.invalid => {
                self.last_round = round;
                Some(convert::round_result(&self.last_round))
            }
            _ => None,
        }
    }

    /// The commands of the mode for the server.
    fn commands(&mut self, commands: Vec<wit::Command>) -> Vec<DriverCommand> {
        let mut commands = convert::commands(commands, &self.playlist);
        if commands
            .iter()
            .any(|command| matches!(command, DriverCommand::SetNextMap(_)))
        {
            self.next_map_chosen = true;
        }
        // Repeated permissions only cost calls to the server.
        commands.retain(|command| match command {
            DriverCommand::SetCanSpawn(DriverCanSpawn {
                account_id,
                can_spawn,
            }) => {
                self.spawn_permissions
                    .insert(account_id.clone(), *can_spawn)
                    != Some(*can_spawn)
            }
            _ => true,
        });
        commands
    }

    /// Unless the mode chose, the playlist goes on in order and a new match starts
    /// with its first map.
    fn default_next_map(
        &self,
        event: &wit::Event,
        commands: &[DriverCommand],
    ) -> Option<DriverCommand> {
        if self.next_map_chosen || self.playlist.is_empty() {
            return None;
        }
        let index = if commands.contains(&DriverCommand::EndMatch) {
            0
        } else if let wit::Event::MapStart = event {
            self.next_map_index()
        } else {
            return None;
        };
        Some(DriverCommand::SetNextMap(self.playlist[index].clone()))
    }

    /// The map after the current one in the playlist, the first one if the current
    /// map is not part of it.
    fn next_map_index(&self) -> usize {
        let current = self
            .map
            .as_ref()
            .and_then(|map| self.playlist.iter().position(|uid| *uid == map.uid));
        match current {
            Some(index) => (index + 1) % self.playlist.len(),
            None => 0,
        }
    }

    /// A reply that only publishes what changed.
    fn published(&mut self) -> DriverReply {
        DriverReply {
            commands: self.publish(),
            ..DriverReply::default()
        }
    }

    /// Publishes what changed for the manialinks. The players come first so the
    /// standings can show their names right away.
    fn publish(&mut self) -> Vec<DriverCommand> {
        let mut commands = Vec::new();
        if let Some(users) = changed(self.users(), &mut self.published.users) {
            commands.push(DriverCommand::SetUsers(json(users)));
        }
        if let Some(standings) = changed(self.standings(), &mut self.published.standings) {
            commands.push(DriverCommand::SetStandings(json(standings)));
        }
        if let Some(round) = changed(self.round(), &mut self.published.round) {
            commands.push(DriverCommand::SetRound(json(round)));
        }
        commands
    }

    fn prepare_call(&mut self) {
        // Setting fuel only fails if fuel is not enabled, which the engine always does.
        _ = self.store.set_fuel(self.fuel_per_call);
        self.store.set_epoch_deadline(self.epoch_deadline);
        self.store.data_mut().log_budget = LOG_BUDGET;
    }
}

/// Clears what belongs to a new match or map and counts the ending round or map
/// before the mode gets the event, so every mode has the same lifecycle of its
/// standings. The round is only output.
fn clear(standings: &mut wit::Standings, event: &wit::Event) {
    standings.round.clear();
    match event {
        wit::Event::MatchStart => *standings = convert::empty_standings(),
        wit::Event::MapStart => standings.rounds_on_map = 0,
        wit::Event::RoundEnd(round_end) if !round_end.invalid => {
            standings.rounds_played = standings.rounds_played.saturating_add(1);
        }
        wit::Event::MapEnd => standings.maps_played = standings.maps_played.saturating_add(1),
        wit::Event::RoundStart
        | wit::Event::RaceStart(_)
        | wit::Event::RoundEnd(_)
        | wit::Event::MatchEnd
        | wit::Event::SettingsChanged => {}
    }
}

/// What the host publishes for the manialinks, as opposed to what a mode decided.
fn is_published(command: &DriverCommand) -> bool {
    matches!(
        command,
        DriverCommand::SetStandings(_) | DriverCommand::SetRound(_) | DriverCommand::SetUsers(_)
    )
}

/// The commands of settings the driver took before an input go out first.
fn after_settings(mut settings: Vec<DriverCommand>, mut reply: DriverReply) -> DriverReply {
    if !settings.is_empty() {
        settings.append(&mut reply.commands);
        reply.commands = settings;
    }
    reply
}

/// The value if it differs from what was published last, which it replaces then.
fn changed<T: PartialEq>(value: T, published: &mut Option<T>) -> Option<&T> {
    if published.as_ref() == Some(&value) {
        return None;
    }
    Some(&*published.insert(value))
}

fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).expect("Only types json can represent are published")
}

struct HostState {
    limits: StoreLimits,
    log_budget: u32,
    /// The players the host knows, for `host.find-user`.
    users: BTreeMap<String, wit::User>,
    /// For `host.map-count`.
    map_count: u32,
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

    fn find_user(&mut self, account_id: String) -> Option<wit::User> {
        self.users.get(&account_id).cloned()
    }

    fn map_count(&mut self) -> u32 {
        self.map_count
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
