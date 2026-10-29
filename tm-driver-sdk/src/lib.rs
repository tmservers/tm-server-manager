//! Write wasm modes for the driver of tm-server-manager.
//!
//! Implement [`Mode`] and export it with [`export_mode!`]. Build the crate as a
//! `cdylib` for `wasm32-unknown-unknown`. The driver host turns the resulting
//! core module into a component when it loads it.
//!
//! The scores of a match live in [`Standings`] every mode shares. The host keeps
//! them and hands them to the mode with every event, so a mode keeps nothing but
//! its settings. The host also shows the standings to the players and reports
//! them to the manager, a mode only applies its rules to them. What the host
//! knows about the players and the match, e.g. names, comes from [`user`] and
//! [`map_count`].
//!
//! Modes run without WASI. Everything they know comes from their settings, the
//! standings, the [`Event`]s and the host functions, which a replay of the match
//! repeats exactly, so it rebuilds the same standings. A mode must not depend on
//! wall clock time or randomness to stay replayable.
//!
//! Settings can change during a match, the ones [`Mode::RELOADABLE`] lists. The
//! SDK rejects new settings that change any other one, and the mode gets
//! [`Event::SettingsChanged`] once it took them.

// Lets the generated bindings and the export macro use the same paths inside
// this crate and inside the mode crates.
extern crate self as tm_driver_sdk;

#[doc(hidden)]
pub use wit_bindgen as __wit_bindgen;

#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "../tm-wasm-driver/wit",
        world: "mode",
        pub_export_macro: true,
        export_macro_name: "__export_mode",
        default_bindings_module: "tm_driver_sdk::bindings",
        runtime_path: "tm_driver_sdk::__wit_bindgen::rt",
        additional_derives: [PartialEq, Eq],
    });
}

mod races;
#[doc(hidden)]
pub mod reload;
pub mod settings;
mod standings;
pub mod testing;

pub use bindings::tmservers::driver::host::LogLevel;
pub use bindings::tmservers::driver::types::{
    CanSpawn, Command, Decision, Entry, Event, FinishTimeout, Label, Layer, Race, Racer, RoundEnd,
    RoundFormat, Standings, Status, Tone, User, WarmUp,
};
pub use races::{CountedRace, race_order};

/// A game mode. One instance lives for the whole match.
pub trait Mode: Sized + 'static {
    /// Shown above the standings, e.g. `Reverse Cup`.
    const NAME: &'static str;

    /// The settings of the match as json. Give the struct `#[serde(default)]` so
    /// every field is optional and `#[serde(deny_unknown_fields)]` so typos are
    /// rejected. It serializes too, so the SDK can tell what new settings change.
    type Settings: serde::de::DeserializeOwned + serde::Serialize;

    /// The settings that can change during a match, by their name in the json, e.g.
    /// a points repartition. New settings that change any other one are rejected,
    /// e.g. the lives players already hold. None by default.
    ///
    /// New settings apply from the next round on, the mode gets
    /// [`Event::SettingsChanged`] once it took them. What happened before stays,
    /// e.g. the points of past rounds.
    const RELOADABLE: &'static [&'static str] = &[];

    /// Creates the mode from the parsed settings of the match. Only validates them
    /// and derives what the mode needs, the parsing is [`Mode::from_json`].
    fn init(settings: Self::Settings) -> Result<Self, String>;

    /// Creates the mode from the json settings of the match. Empty settings are `{}`.
    fn from_json(settings: &str) -> Result<Self, String> {
        reload::Configured::<Self>::new(settings).map(|configured| configured.mode)
    }

    /// Applies the rules of the mode to an event and returns the commands for the
    /// server in order.
    ///
    /// The scores live in `standings`, which the host keeps between events. That is
    /// why a mode only gets `&self`: everything besides its settings belongs into the
    /// standings.
    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command>;
}

/// Logs through the driver host. Prints to stderr when running natively (e.g. in tests).
pub fn log(level: LogLevel, message: &str) {
    #[cfg(target_arch = "wasm32")]
    bindings::tmservers::driver::host::log(level, message);
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("[{level:?}] {message}");
}

/// What the host knows about a player. Natively (e.g. in tests) the users of the
/// [`testing::Harness`].
pub fn user(account_id: &str) -> Option<User> {
    #[cfg(target_arch = "wasm32")]
    let user = bindings::tmservers::driver::host::find_user(account_id);
    #[cfg(not(target_arch = "wasm32"))]
    let user = testing::native::user(account_id);
    user
}

/// The name of a player, e.g. for chat messages. The account id while the host
/// does not know the name.
pub fn name(account_id: &str) -> String {
    user(account_id)
        .map(|user| user.name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| account_id.into())
}

/// A place for chat messages: 1st, 2nd, 3rd, 4th, …, 11th, 12th, 13th, …, 21st, …
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Maps in the playlist of the match, 0 without one. Natively (e.g. in tests) the
/// map count of the [`testing::Harness`].
pub fn map_count() -> u32 {
    #[cfg(target_arch = "wasm32")]
    let count = bindings::tmservers::driver::host::map_count();
    #[cfg(not(target_arch = "wasm32"))]
    let count = testing::native::map_count();
    count
}

/// Exports a [`Mode`] as the entry point of the wasm module.
///
/// Only expands on `wasm32` so the mode can still be built and tested natively.
#[macro_export]
macro_rules! export_mode {
    ($mode:ty) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            ::std::thread_local! {
                static MODE: ::core::cell::RefCell<
                    ::core::option::Option<$crate::reload::Configured<$mode>>,
                > = const { ::core::cell::RefCell::new(::core::option::Option::None) };
            }

            struct Export;

            impl $crate::bindings::Guest for Export {
                fn init(
                    settings: ::std::string::String,
                ) -> ::core::result::Result<(), ::std::string::String> {
                    let configured = $crate::reload::Configured::<$mode>::new(&settings)?;
                    MODE.with(|m| *m.borrow_mut() = ::core::option::Option::Some(configured));
                    ::core::result::Result::Ok(())
                }

                fn name() -> ::std::string::String {
                    ::std::string::String::from(<$mode as $crate::Mode>::NAME)
                }

                fn step(
                    mut standings: $crate::Standings,
                    event: $crate::Event,
                ) -> $crate::Decision {
                    let commands = MODE.with(|m| match m.borrow().as_ref() {
                        ::core::option::Option::Some(configured) => {
                            <$mode as $crate::Mode>::step(&configured.mode, &mut standings, event)
                        }
                        ::core::option::Option::None => {
                            $crate::log($crate::LogLevel::Error, "step was called before init");
                            ::std::vec::Vec::new()
                        }
                    });
                    $crate::Decision {
                        standings,
                        commands,
                    }
                }

                fn check_settings(
                    settings: ::std::string::String,
                ) -> ::core::result::Result<(), ::std::string::String> {
                    MODE.with(|m| match m.borrow().as_ref() {
                        ::core::option::Option::Some(configured) => {
                            configured.change(&settings).map(|_| ())
                        }
                        ::core::option::Option::None => ::core::result::Result::Err(
                            "check-settings was called before init".into(),
                        ),
                    })
                }

                fn change_settings(
                    settings: ::std::string::String,
                ) -> ::core::result::Result<(), ::std::string::String> {
                    MODE.with(|m| {
                        let mut m = m.borrow_mut();
                        let next = match m.as_ref() {
                            ::core::option::Option::Some(configured) => {
                                configured.change(&settings)?
                            }
                            ::core::option::Option::None => {
                                return ::core::result::Result::Err(
                                    "change-settings was called before init".into(),
                                );
                            }
                        };
                        *m = ::core::option::Option::Some(next);
                        ::core::result::Result::Ok(())
                    })
                }

                fn reloadable_settings() -> ::std::vec::Vec<::std::string::String> {
                    <$mode as $crate::Mode>::RELOADABLE
                        .iter()
                        .map(|name| ::std::string::String::from(*name))
                        .collect()
                }
            }

    $crate::bindings::__export_mode!(Export with_types_in $crate::bindings);
        };
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals() {
        let ordinals: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            ordinals,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
    }
}
