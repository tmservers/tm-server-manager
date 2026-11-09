//! Modes written as Rhai scripts, e.g. by the organizers of a competition.
//!
//! A script is the rules of a mode like a built-in one: it gets the same events
//! and changes the same standings, so everything else, the manialinks, the
//! manager and the simulator, works with it unchanged. It defines functions the
//! driver calls, everything else of it is ignored:
//!
//! ```rhai
//! fn name() { "Sprint" }
//! // The settings with their defaults. Settings not listed here are rejected.
//! fn defaults() { #{ points: [3, 2, 1], rounds: 5 } }
//! // The settings which can change during a match, none without it.
//! fn reloadable() { ["points"] }
//! // Throws to reject settings the defaults cannot tell apart from valid ones.
//! fn validate(settings) { if settings.rounds < 1 { throw "rounds has to be 1 at least"; } }
//! // Changes the standings and returns the commands for the server.
//! fn step(cx, standings, event) { [] }
//! // The round as it stands while it runs, nothing without it.
//! fn live_round(cx, standings, round) { [] }
//! ```
//!
//! Only `step` is required. Scripts keep nothing between calls: what they know
//! are their settings (`cx.settings`), the standings and the event, so a replay of
//! a match calls them the same way and gets the same answers. They cannot read a
//! clock, random numbers or anything outside of the match.
//!
//! Every call gets a budget of [`BUDGET`] operations. A script which runs out of
//! it, or fails in any other way, faults: the driver stops using it and the
//! modescript shows that it waits. Nobody ties in scripts either: `rank`,
//! `place_round` and `race_order` break every tie like the built-in modes.

mod api;
mod settings;

use std::{cell::Cell, cell::RefCell, collections::BTreeMap, rc::Rc};

use rhai::{
    AST, Array, CallFnOptions, Dynamic, Engine, EvalAltResult, FuncArgs, ImmutableString, Map,
    Scope,
    packages::{BasicArrayPackage, BasicMapPackage, CorePackage, LogicPackage, Package},
};
use tm_mode_driver_sdk::{
    Command, Decision, Entry, Event, Host, LogLevel, ModeFault, ModeInstance, ModeSource, RoundEnd,
    Standings, User,
};

/// The operations a call into a script may run before it faults, e.g. because it never ends.
pub const BUDGET: u64 = 2_000_000;

/// The function every script has to define.
const STEP: &str = "step";
const LIVE_ROUND: &str = "live_round";
const NAME: &str = "name";
const DEFAULTS: &str = "defaults";
const RELOADABLE: &str = "reloadable";
const VALIDATE: &str = "validate";

/// The functions the driver calls with the number of arguments they get.
const HOOKS: [(&str, usize); 6] = [
    (STEP, 3),
    (LIVE_ROUND, 3),
    (NAME, 0),
    (DEFAULTS, 0),
    (RELOADABLE, 0),
    (VALIDATE, 1),
];

/// A mode written as a script.
#[derive(Debug, Clone)]
pub struct ScriptMode {
    source: String,
}

impl ScriptMode {
    /// The mode of a script, an error tells why the script cannot be one, e.g.
    /// for whoever saves it.
    pub fn new(source: impl Into<String>) -> Result<Self, String> {
        let source = source.into();
        compile(&compiler(), &source)?;
        Ok(ScriptMode { source })
    }
}

impl ModeSource for ScriptMode {
    fn start(&self, settings: &str) -> Result<Result<Box<dyn ModeInstance>, String>, ModeFault> {
        let cost = Rc::new(Cell::new(0));
        let printed = Rc::new(RefCell::new(Vec::new()));
        let engine = engine(cost.clone(), printed.clone());
        // It compiled when the mode was made.
        let ast = compile(&engine, &self.source).map_err(ModeFault)?;
        let mut script = Script {
            engine,
            ast,
            cost,
            printed,
            name: "Script".into(),
            defaults: Map::new(),
            reloadable: Vec::new(),
            settings: Map::new(),
        };

        if script.defines(NAME) {
            script.name = script.call::<ImmutableString>(NAME, ())?.into();
        }
        if script.defines(DEFAULTS) {
            script.defaults = script.call::<Map>(DEFAULTS, ())?;
        }
        if script.defines(RELOADABLE) {
            script.reloadable = script
                .call::<Array>(RELOADABLE, ())?
                .into_iter()
                .map(|name| {
                    name.into_string().map_err(|kind| {
                        ModeFault(format!(
                            "{RELOADABLE} has to return the names of settings, not a {kind}."
                        ))
                    })
                })
                .collect::<Result<_, _>>()?;
        }

        Ok(match script.configured(settings)? {
            Ok(settings) => {
                script.settings = settings;
                Ok(Box::new(script))
            }
            Err(rejection) => Err(rejection),
        })
    }
}

/// A script running for one match.
struct Script {
    engine: Engine,
    ast: AST,
    /// The operations of the call that runs or ran last.
    cost: Rc<Cell<u64>>,
    /// What the script printed in the call that runs.
    printed: Rc<RefCell<Vec<String>>>,
    name: String,
    defaults: Map,
    reloadable: Vec<String>,
    /// The settings the match plays with, every default filled in.
    settings: Map,
}

impl Script {
    fn defines(&self, hook: &str) -> bool {
        self.ast.iter_functions().any(|f| f.name == hook)
    }

    /// Calls a function of the script with a budget of its own.
    fn call<T: Clone + 'static>(&self, hook: &str, args: impl FuncArgs) -> Result<T, ModeFault> {
        self.cost.set(0);
        self.engine
            .call_fn_with_options::<T>(
                // Only the functions of the script are used.
                CallFnOptions::new().eval_ast(false),
                &mut Scope::new(),
                &self.ast,
                hook,
                args,
            )
            .map_err(|error| ModeFault(format!("{hook} of the script failed: {error}")))
    }

    /// The settings a match would play with, or why the script rejects them.
    fn configured(&self, json: &str) -> Result<Result<Map, String>, ModeFault> {
        let given = match settings::parse(&self.engine, json) {
            Ok(given) => given,
            Err(rejection) => return Ok(Err(rejection)),
        };
        // Without declared defaults the script takes whatever it gets.
        let settings = if self.defines(DEFAULTS) {
            match settings::merge(&self.defaults, given) {
                Ok(settings) => settings,
                Err(rejection) => return Ok(Err(rejection)),
            }
        } else {
            given
        };
        if self.defines(VALIDATE) {
            self.cost.set(0);
            let validated = self.engine.call_fn_with_options::<Dynamic>(
                CallFnOptions::new().eval_ast(false),
                &mut Scope::new(),
                &self.ast,
                VALIDATE,
                (settings.clone(),),
            );
            match validated {
                Ok(_) => {}
                // What the script throws is why it rejects the settings.
                Err(error) => match *error {
                    EvalAltResult::ErrorRuntime(reason, _) => {
                        return Ok(Err(format!("Invalid settings: {reason}")));
                    }
                    error => {
                        return Err(ModeFault(format!(
                            "{VALIDATE} of the script failed: {error}"
                        )));
                    }
                },
            }
        }
        Ok(Ok(settings))
    }

    /// The settings a match would change to, or why they cannot replace the current ones.
    fn changed(&self, json: &str) -> Result<Result<Map, String>, ModeFault> {
        let next = match self.configured(json)? {
            Ok(next) => next,
            Err(rejection) => return Ok(Err(rejection)),
        };
        let fixed = settings::fixed_changes(&self.settings, &next, &self.reloadable);
        Ok(if fixed.is_empty() {
            Ok(next)
        } else {
            Err(format!("{} cannot change during a match", fixed.join(", ")))
        })
    }

    /// What the script can ask the host for during a call about these players.
    fn context<'a>(
        &self,
        host: &dyn Host,
        players: impl IntoIterator<Item = &'a str>,
    ) -> api::Context {
        let users: BTreeMap<String, User> = players
            .into_iter()
            .filter_map(|account_id| Some((account_id.to_string(), host.user(account_id)?)))
            .collect();
        api::Context {
            settings: self.settings.clone(),
            users: Rc::new(users),
            map_count: host.map_count(),
            logs: Rc::default(),
        }
    }

    /// Hands what the script printed and logged to the host.
    fn flush(&self, host: &dyn Host, cx: &api::Context) {
        for line in self.printed.borrow_mut().drain(..) {
            host.log(LogLevel::Info, &line);
        }
        for (level, line) in cx.logs.borrow_mut().drain(..) {
            host.log(level, &line);
        }
    }
}

impl ModeInstance for Script {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn reloadable_settings(&self) -> Vec<String> {
        self.reloadable.clone()
    }

    fn step(
        &mut self,
        host: &dyn Host,
        standings: &Standings,
        event: Event,
    ) -> Result<Decision, ModeFault> {
        let cx = self.context(host, players(standings, round_of(&event), &event));
        let shared = api::Standings::new(standings.clone());
        let returned =
            self.call::<Array>(STEP, (cx.clone(), shared.clone(), api::Event::from(event)));
        self.flush(host, &cx);

        let commands = returned?
            .into_iter()
            .map(|value| {
                let kind = value.type_name();
                value.try_cast::<Command>().ok_or_else(|| {
                    ModeFault(format!(
                        "{STEP} of the script returned a {kind} instead of a command."
                    ))
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Decision {
            standings: shared.get(),
            commands,
        })
    }

    fn live_round(
        &mut self,
        host: &dyn Host,
        standings: &Standings,
        races: &RoundEnd,
    ) -> Result<Vec<Entry>, ModeFault> {
        if !self.defines(LIVE_ROUND) {
            return Ok(Vec::new());
        }
        let cx = self.context(host, players(standings, Some(races), &Event::RoundStart));
        // A copy, so the live round cannot change the match.
        let shared = api::Standings::new(standings.clone());
        let round = api::Round(Rc::new(races.clone()));
        let returned = self.call::<Array>(LIVE_ROUND, (cx.clone(), shared, round));
        self.flush(host, &cx);

        returned?
            .into_iter()
            .map(|value| {
                let kind = value.type_name();
                value.try_cast::<Entry>().ok_or_else(|| {
                    ModeFault(format!(
                        "{LIVE_ROUND} of the script returned a {kind} instead of an entry."
                    ))
                })
            })
            .collect()
    }

    fn check_settings(&mut self, settings: &str) -> Result<Result<(), String>, ModeFault> {
        Ok(self.changed(settings)?.map(|_| ()))
    }

    fn change_settings(&mut self, settings: &str) -> Result<Result<(), String>, ModeFault> {
        Ok(self.changed(settings)?.map(|next| self.settings = next))
    }

    fn last_cost(&self) -> Option<u64> {
        Some(self.cost.get())
    }

    fn budget(&self) -> Option<u64> {
        Some(BUDGET)
    }
}

fn round_of(event: &Event) -> Option<&RoundEnd> {
    match event {
        Event::RoundEnd(round) => Some(round),
        _ => None,
    }
}

/// Everyone a call is about: the players of the standings, of the round and of the event.
fn players<'a>(
    standings: &'a Standings,
    round: Option<&'a RoundEnd>,
    event: &'a Event,
) -> impl Iterator<Item = &'a str> {
    let started = match event {
        Event::RaceStart(account_id) => Some(account_id.as_str()),
        _ => None,
    };
    let racing = round.into_iter().flat_map(|round| {
        round
            .racers
            .iter()
            .map(|racer| racer.account_id.as_str())
            .chain(round.present.iter().map(String::as_str))
    });
    standings
        .entries
        .iter()
        .map(|entry| entry.account_id.as_str())
        .chain(racing)
        .chain(started)
}

/// The script compiled. It has to define `step`, and the functions the driver
/// calls have to take the arguments they get.
fn compile(engine: &Engine, source: &str) -> Result<AST, String> {
    let ast = engine
        .compile(source)
        .map_err(|error| format!("The script does not compile: {error}"))?;
    for (hook, arguments) in HOOKS {
        match ast.iter_functions().find(|f| f.name == hook) {
            Some(function) if function.params.len() != arguments => {
                return Err(format!(
                    "`{hook}` of the script has to take {arguments} arguments, not {}.",
                    function.params.len()
                ));
            }
            None if hook == STEP => {
                return Err(format!(
                    "The script has to define `fn {STEP}(cx, standings, event)`."
                ));
            }
            _ => {}
        }
    }
    Ok(ast)
}

/// An engine which can only compile scripts, compiling does not need what is registered.
fn compiler() -> Engine {
    let mut engine = Engine::new_raw();
    engine
        .set_max_operations(BUDGET)
        .set_max_call_levels(32)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(4096)
        .set_max_array_size(100_000)
        .set_max_map_size(1_000)
        // Typos are errors instead of silently being nothing.
        .set_strict_variables(true)
        .set_fail_on_invalid_map_property(true);
    engine
}

/// The engine the script of a match runs in.
fn engine(cost: Rc<Cell<u64>>, printed: Rc<RefCell<Vec<String>>>) -> Engine {
    let mut engine = compiler();
    engine.register_global_module(CorePackage::new().as_shared_module());
    engine.register_global_module(LogicPackage::new().as_shared_module());
    engine.register_global_module(BasicArrayPackage::new().as_shared_module());
    engine.register_global_module(BasicMapPackage::new().as_shared_module());

    // The same on every machine, so it tells how far a call stays below its budget.
    engine.on_progress(move |operations| {
        cost.set(operations);
        None
    });
    let debugged = printed.clone();
    engine.on_print(move |text| printed.borrow_mut().push(text.into()));
    engine.on_debug(move |text, _, position| {
        debugged.borrow_mut().push(format!("{position}: {text}"));
    });

    api::register(&mut engine);
    engine
}
