//! A mode as the driver host runs it, whatever runs its rules.

use std::{
    fmt,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind},
};

use crate::{Decision, Entry, Event, Host, Mode, RoundEnd, Standings, reload::Configured};

/// A mode failed in a way it cannot recover from, e.g. a script ran out of its
/// budget or a built-in mode panicked. The host stops using the mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeFault(pub String);

impl fmt::Display for ModeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModeFault {}

/// A mode running for one match.
///
/// The host keeps the standings and hands them over with every call, so an
/// instance keeps nothing but its settings. With the same settings and the same
/// calls it has to answer the same, a replay of the match relies on it.
pub trait ModeInstance {
    /// Name of the mode, shown above the standings.
    fn name(&self) -> String;

    /// The settings that can change during a match, by their name in the json.
    fn reloadable_settings(&self) -> Vec<String>;

    /// Called for every event with the standings the mode returned last.
    fn step(
        &mut self,
        host: &dyn Host,
        standings: &Standings,
        event: Event,
    ) -> Result<Decision, ModeFault>;

    /// The round as it stands while it runs, placed as if it ended now with the
    /// races so far. It changes nothing, the round end still decides.
    fn live_round(
        &mut self,
        host: &dyn Host,
        standings: &Standings,
        races: &RoundEnd,
    ) -> Result<Vec<Entry>, ModeFault>;

    /// Whether new json settings could replace the ones of the match during it,
    /// without taking them. The inner error is why the mode rejects them.
    fn check_settings(&mut self, settings: &str) -> Result<Result<(), String>, ModeFault>;

    /// Replaces the settings of the match during it if [`ModeInstance::check_settings`]
    /// accepts them, and keeps the ones it has otherwise. The host tells the mode
    /// with [`Event::SettingsChanged`] afterwards.
    fn change_settings(&mut self, settings: &str) -> Result<Result<(), String>, ModeFault>;

    /// What the last call cost, for engines which give every call a budget, e.g.
    /// the operations of a script. The same for the same call on every machine.
    fn last_cost(&self) -> Option<u64> {
        None
    }

    /// The most a call may cost before the engine stops it with a fault.
    fn budget(&self) -> Option<u64> {
        None
    }
}

/// Where the modes of matches come from: a built-in mode or a compiled script.
pub trait ModeSource {
    /// Starts the mode for a match with its json settings. The inner error is why
    /// the mode rejects the settings.
    fn start(&self, settings: &str) -> Result<Result<Box<dyn ModeInstance>, String>, ModeFault>;
}

/// A [`Mode`] compiled into the driver.
pub struct Native<M>(PhantomData<fn() -> M>);

impl<M> Native<M> {
    pub const fn new() -> Self {
        Native(PhantomData)
    }
}

impl<M> Default for Native<M> {
    fn default() -> Self {
        Native::new()
    }
}

impl<M: Mode> ModeSource for Native<M> {
    fn start(&self, settings: &str) -> Result<Result<Box<dyn ModeInstance>, String>, ModeFault> {
        guarded(|| {
            Configured::<M>::new(settings)
                .map(|configured| Box::new(configured) as Box<dyn ModeInstance>)
        })
    }
}

impl<M: Mode> ModeInstance for Configured<M> {
    fn name(&self) -> String {
        M::NAME.into()
    }

    fn reloadable_settings(&self) -> Vec<String> {
        M::RELOADABLE.iter().map(|name| (*name).into()).collect()
    }

    fn step(
        &mut self,
        host: &dyn Host,
        standings: &Standings,
        event: Event,
    ) -> Result<Decision, ModeFault> {
        guarded(|| {
            let mut standings = standings.clone();
            let commands = self.mode.step(host, &mut standings, event);
            Decision {
                standings,
                commands,
            }
        })
    }

    fn live_round(
        &mut self,
        host: &dyn Host,
        standings: &Standings,
        races: &RoundEnd,
    ) -> Result<Vec<Entry>, ModeFault> {
        guarded(|| self.mode.live_round(host, standings, races))
    }

    fn check_settings(&mut self, settings: &str) -> Result<Result<(), String>, ModeFault> {
        guarded(|| self.change(settings).map(|_| ()))
    }

    fn change_settings(&mut self, settings: &str) -> Result<Result<(), String>, ModeFault> {
        guarded(|| {
            *self = self.change(settings)?;
            Ok(())
        })
    }
}

/// A panic of a built-in mode is a bug of it. It must not take the driver of
/// other matches down with it, so it becomes a fault like a failing script.
fn guarded<T>(call: impl FnOnce() -> T) -> Result<T, ModeFault> {
    catch_unwind(AssertUnwindSafe(call)).map_err(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(|message| (*message).to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        ModeFault(format!("The mode panicked: {message}"))
    })
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{Command, testing::TestHost};

    #[derive(Debug, Default, Deserialize, Serialize)]
    #[serde(default, deny_unknown_fields)]
    struct Settings {
        points: i32,
        panics: bool,
    }

    struct Test(Settings);

    impl Mode for Test {
        const NAME: &'static str = "Test";
        const RELOADABLE: &'static [&'static str] = &["points"];

        type Settings = Settings;

        fn init(settings: Settings) -> Result<Self, String> {
            Ok(Self(settings))
        }

        fn step(&self, host: &dyn Host, standings: &mut Standings, _: Event) -> Vec<Command> {
            assert!(!self.0.panics, "told to");
            standings.rounds_on_map = host.map_count();
            standings.award("a", self.0.points);
            vec![Command::EndMap]
        }
    }

    fn start(settings: &str) -> Box<dyn ModeInstance> {
        Native::<Test>::new()
            .start(settings)
            .expect("starting does not fault")
            .expect("the settings are valid")
    }

    #[test]
    fn a_built_in_mode_steps_on_a_copy_of_the_standings_it_gets() {
        let host = TestHost {
            map_count: 3,
            ..TestHost::default()
        };
        let mut mode = start(r#"{"points": 5}"#);
        let standings = Standings::default();

        let decision = mode.step(&host, &standings, Event::RoundStart).unwrap();

        assert_eq!(standings, Standings::default());
        assert_eq!(decision.standings.rounds_on_map, 3);
        assert_eq!(decision.standings.player("a").map(|a| a.score), Some(5));
        assert_eq!(decision.commands, vec![Command::EndMap]);
        assert_eq!(
            (mode.name(), mode.reloadable_settings()),
            ("Test".into(), vec!["points".to_string()])
        );
    }

    #[test]
    fn settings_a_mode_rejects_are_no_fault() {
        let rejected = Native::<Test>::new().start(r#"{"lives": 1}"#).unwrap();

        assert!(rejected.is_err_and(|error| error.contains("lives")));
    }

    #[test]
    fn checking_settings_does_not_take_them() {
        let host = TestHost::default();
        let mut mode = start(r#"{"points": 5}"#);

        assert_eq!(mode.check_settings(r#"{"points": 7}"#), Ok(Ok(())));
        let checked = mode
            .step(&host, &Standings::default(), Event::RoundStart)
            .unwrap();
        assert_eq!(mode.change_settings(r#"{"points": 7}"#), Ok(Ok(())));
        let changed = mode
            .step(&host, &Standings::default(), Event::RoundStart)
            .unwrap();

        assert_eq!(checked.standings.player("a").map(|a| a.score), Some(5));
        assert_eq!(changed.standings.player("a").map(|a| a.score), Some(7));
    }

    #[test]
    fn settings_which_cannot_change_keep_the_ones_of_the_match() {
        let host = TestHost::default();
        let mut mode = start(r#"{"points": 5}"#);

        let rejected = mode
            .change_settings(r#"{"points": 7, "panics": true}"#)
            .unwrap();

        assert_eq!(rejected, Err("panics cannot change during a match".into()));
        let decision = mode
            .step(&host, &Standings::default(), Event::RoundStart)
            .unwrap();
        assert_eq!(decision.standings.player("a").map(|a| a.score), Some(5));
    }

    #[test]
    fn a_panic_of_a_built_in_mode_is_a_fault() {
        let mut mode = start(r#"{"panics": true}"#);

        let fault = mode
            .step(
                &TestHost::default(),
                &Standings::default(),
                Event::RoundStart,
            )
            .unwrap_err();

        assert_eq!(fault, ModeFault("The mode panicked: told to".into()));
    }
}
