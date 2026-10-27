//! Runs wasm modes that drive the generic driver modescript of a Trackmania server.
//!
//! A mode is a component implementing the `mode` world of `wit/driver.wit`.
//! The host keeps it in a sandbox: no WASI, a fuel and a wall clock budget for
//! every call and a memory limit.
//!
//! Modes are fed from the standard callbacks of the server: the host builds the
//! race results of a round from the waypoints and keeps the players it knows. On
//! top of that the host handles the [`DriverHold`]s of the modescript: it steps
//! the mode, lets the hold continue and answers repeated holds without stepping
//! the mode twice.
//!
//! The scores of a match live in standings every mode shares. The host keeps
//! them and hands them to the mode with every event, so modes keep nothing but
//! their settings. The host clears what belongs to a new match or map, publishes
//! the standings, the last round and the players for the manialinks whenever
//! they change and reports every round the mode placed to the manager.
//!
//! Modes and the host are deterministic. Replaying the events of a match with
//! [`DriverEngine::replay`] rebuilds exactly the same driver, so a crashed bridge
//! resumes from the event stream and nothing else has to be persisted.

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
    event::{DriverHold, DriverRoundResult, DriverSection, Event},
    method::{DriverCanSpawn, DriverCommand, DriverRound, DriverStandings, DriverUsers},
};
use wasmtime::{
    Config, Engine, Store, StoreLimits, StoreLimitsBuilder,
    component::{Component, HasSelf, Linker},
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
        config.strategy(wasmtime::Strategy::Cranelift);
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        config.epoch_interruption(true);
        // Enable determinsim
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
        Ok(driver)
    }

    // Restore Match state from a Event stream.
    pub fn replay<'a>(
        &self,
        component: &Component,
        settings: &str,
        playlist: &[String],
        events: impl IntoIterator<Item = &'a Event>,
    ) -> Result<Driver, DriverError> {
        let mut driver = self.instantiate(component, settings)?;
        driver.set_playlist(playlist.iter().cloned());
        for event in events {
            driver.handle(event)?;
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
            log_budget: 64,
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
            name: String::new(),
            standings: convert::empty_standings(),
            last_round: Vec::new(),
            round_number: 0,
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
    /// Name of the mode, shown above the standings.
    name: String,
    /// The scores of the match, handed to the mode with every event.
    standings: wit::Standings,
    /// The last round that counted, as the mode placed it.
    last_round: Vec<wit::Entry>,
    /// Rounds of the match that counted.
    round_number: u32,
    published: Published,
    /// Spawn permissions sent in this match, so repeated ones are left out.
    spawn_permissions: BTreeMap<String, bool>,
    session: Option<i32>,
    /// Commands for the last hold in case the modescript asks again.
    last_hold: Option<(u32, Vec<DriverCommand>)>,
    /// Uids of the maps the match plays in order.
    playlist: Vec<String>,
    /// Uid and name of the current map.
    map: Option<(String, String)>,
    /// The mode chose the next map since the current one started.
    next_map_chosen: bool,
    /// Players on the server, who take part in round results without racing.
    connected: BTreeSet<String>,
    /// A round runs, from its start to its end hold. Warmup races are outside.
    in_round: bool,
    /// The built-in warmup runs.
    in_warm_up: bool,
    /// The races of the current round.
    races: Races,
    poisoned: bool,
}

/// What the manialinks got last, so only changes are sent.
#[derive(Default)]
struct Published {
    standings: Option<String>,
    round: Option<String>,
    users: Option<String>,
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
            Event::StartMatchStart(_) => {
                // Every match starts with the defaults of the modescript.
                self.spawn_permissions.clear();
                self.last_round.clear();
                self.round_number = 0;
                // Everything is published again, e.g. for a modescript that just started.
                self.published = Published::default();
                self.step(wit::Event::MatchStart)
            }
            Event::StartMapStart(start) => {
                self.map = Some((start.map.uid.clone(), start.map.name.clone()));
                self.next_map_chosen = false;
                Ok(DriverReply::default())
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
                self.step(wit::Event::RoundStart)
            }
            Event::StartLine(start_line) => {
                let first = self.races.start(&start_line.account_id);
                // Time attack rounds start players again and again.
                if !first || !self.in_round || self.in_warm_up {
                    return Ok(DriverReply::default());
                }
                self.step(wit::Event::RaceStart(start_line.account_id.clone()))
            }
            Event::WayPoint(way_point) => {
                self.races.way_point(way_point);
                Ok(DriverReply::default())
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
                // The server shows up as a player too.
                if info.is_server() {
                    return Ok(DriverReply::default());
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
                Ok(DriverReply {
                    commands: self.publish(),
                    round_result: None,
                })
            }
            Event::EndMatchStart(_) => self.step(wit::Event::MatchEnd),
            Event::DriverHold(hold) => self.hold(hold),
            _ => Ok(DriverReply::default()),
        }
    }

    fn hold(&mut self, hold: &DriverHold) -> Result<DriverReply, DriverError> {
        // A new session means the modescript restarted and counts from the start again.
        if self.session != Some(hold.session) {
            self.session = Some(hold.session);
            self.last_hold = None;
            self.spawn_permissions.clear();
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

        // The script waits between rounds. A round end without its start, e.g. after the
        // bridge restarted, missed races, so it does not count.
        let in_round = std::mem::take(&mut self.in_round);
        let event = match hold.section {
            DriverSection::MapStart => wit::Event::MapStart,
            DriverSection::RoundEnd => wit::Event::RoundEnd(wit::RoundEnd {
                forced: hold.forced || !in_round,
                results: self.races.results(&self.connected),
            }),
            DriverSection::MapEnd => wit::Event::MapEnd,
        };

        let mut reply = self.step(event)?;
        reply.commands.push(DriverCommand::Continue(hold.seq));
        self.last_hold = Some((hold.seq, reply.commands.clone()));
        Ok(reply)
    }

    /// The standings of the match as the mode left them, ordered by position.
    pub fn standings(&self) -> DriverStandings {
        let map_name = self.map.as_ref().map_or("", |(_, name)| name.as_str());
        convert::standings(
            &self.name,
            map_name,
            self.playlist.len() as u32,
            &self.standings,
        )
    }

    /// The last round that counted, ordered by place.
    pub fn last_round(&self) -> DriverRound {
        convert::round(self.round_number, &self.last_round)
    }

    /// The players the host knows, e.g. for their names.
    pub fn users(&self) -> DriverUsers {
        convert::users(&self.store.data().users)
    }

    /// The uids of the maps the match plays, e.g. the map pool of the config. The maps
    /// have to be in the map list of the server. Set it before the first event, a
    /// replay needs the same playlist.
    pub fn set_playlist(&mut self, maps: impl IntoIterator<Item = impl Into<String>>) {
        self.playlist = maps.into_iter().map(Into::into).collect();
    }

    fn step(&mut self, event: wit::Event) -> Result<DriverReply, DriverError> {
        clear(&mut self.standings, &event);
        // The mode reads it through the host functions.
        self.store.data_mut().map_count = self.playlist.len() as u32;

        self.prepare_call();
        let decision = match self
            .bindings
            .call_step(&mut self.store, &self.standings, &event)
        {
            Ok(decision) => decision,
            Err(error) => {
                self.poisoned = true;
                return Err(DriverError::Trap(error));
            }
        };
        self.standings = decision.standings;

        // The mode reports the round and never gets it back.
        let round = std::mem::take(&mut self.standings.round);
        let round_result = match &event {
            wit::Event::RoundEnd(round_end) if !round_end.forced => {
                self.round_number += 1;
                self.last_round = round;
                Some(convert::round_result(&self.last_round))
            }
            _ => None,
        };

        let mut commands = convert::commands(decision.commands, &self.playlist);
        commands.retain(|command| match command {
            DriverCommand::SetNextMap(_) => {
                self.next_map_chosen = true;
                true
            }
            // Repeated permissions only cost calls to the server.
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

        // Unless the mode chose, the playlist goes on in order and a new match starts
        // with its first map.
        if !self.next_map_chosen && !self.playlist.is_empty() {
            let next = if commands.contains(&DriverCommand::EndMatch) {
                Some(0)
            } else if let wit::Event::MapStart = event {
                Some(self.next_map_index())
            } else {
                None
            };
            if let Some(index) = next {
                commands.push(DriverCommand::SetNextMap(self.playlist[index].clone()));
            }
        }

        commands.extend(self.publish());

        Ok(DriverReply {
            commands,
            round_result,
        })
    }

    /// Publishes what changed for the manialinks. The players come first so the
    /// standings can show their names right away.
    fn publish(&mut self) -> Vec<DriverCommand> {
        let mut commands = Vec::new();

        let users = json(&self.users());
        if self.published.users.as_ref() != Some(&users) {
            self.published.users = Some(users.clone());
            commands.push(DriverCommand::SetUsers(users));
        }
        let standings = json(&self.standings());
        if self.published.standings.as_ref() != Some(&standings) {
            self.published.standings = Some(standings.clone());
            commands.push(DriverCommand::SetStandings(standings));
        }
        let round = json(&self.last_round());
        if self.published.round.as_ref() != Some(&round) {
            self.published.round = Some(round.clone());
            commands.push(DriverCommand::SetRound(round));
        }

        commands
    }

    /// The map after the current one in the playlist, the first one if the current
    /// map is not part of it.
    fn next_map_index(&self) -> usize {
        let current = self
            .map
            .as_ref()
            .and_then(|(uid, _)| self.playlist.iter().position(|map| map == uid));
        match current {
            Some(index) => (index + 1) % self.playlist.len(),
            None => 0,
        }
    }

    fn prepare_call(&mut self) {
        // Setting fuel only fails if fuel is not enabled, which the engine always does.
        _ = self.store.set_fuel(self.fuel_per_call);
        self.store.set_epoch_deadline(self.epoch_deadline);
        self.store.data_mut().log_budget = 64;
    }
}

/// Clears what belongs to a new match or map before the mode gets the event, so
/// every mode has the same lifecycle of its standings. The round is only output.
fn clear(standings: &mut wit::Standings, event: &wit::Event) {
    standings.round.clear();
    match event {
        wit::Event::MatchStart => *standings = convert::empty_standings(),
        wit::Event::MapStart => standings.rounds_on_map = 0,
        wit::Event::MapEnd => standings.maps_played = standings.maps_played.saturating_add(1),
        wit::Event::RoundStart
        | wit::Event::RaceStart(_)
        | wit::Event::RoundEnd(_)
        | wit::Event::MatchEnd => {}
    }
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
