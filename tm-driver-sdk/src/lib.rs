//! Write wasm modes for the driver of tm-server-manager.
//!
//! Implement [`Mode`] and export it with [`export_mode!`]. Build the crate as a
//! `cdylib` for `wasm32-unknown-unknown`. The driver host turns the resulting
//! core module into a component when it loads it.
//!
//! The scores of a match live in [`Standings`] every mode shares. The host keeps
//! them and hands them to the mode with every event, so a mode keeps nothing but
//! its settings. The host also shows the standings to the players and reports
//! them to the manager, a mode only applies its rules to them.
//!
//! Modes run without WASI. Everything they know comes from their settings, the
//! standings and the [`Event`]s, so replaying the events of a match rebuilds the
//! same standings. A mode must not depend on wall clock time or randomness to
//! stay replayable.

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

pub mod settings;
mod standings;
pub mod testing;

pub use bindings::tmservers::driver::host::LogLevel;
pub use bindings::tmservers::driver::types::{
    CanSpawn, Command, Decision, Event, FinishTimeout, Label, Layer, Player, RaceResult, RoundEnd,
    RoundFormat, Standing, Standings, Status, Tone, WarmUp,
};

/// A game mode. One instance lives for the whole match.
pub trait Mode: Sized + 'static {
    /// Shown above the standings, e.g. `Reverse Cup`.
    const NAME: &'static str;

    /// The settings of the match as json. Give the struct `#[serde(default)]` so
    /// every field is optional and `#[serde(deny_unknown_fields)]` so typos are
    /// rejected.
    type Settings: serde::de::DeserializeOwned;

    /// Creates the mode from the parsed settings of the match. Only validates them
    /// and derives what the mode needs, the parsing is [`Mode::from_json`].
    fn init(settings: Self::Settings) -> Result<Self, String>;

    /// Creates the mode from the json settings of the match. Empty settings are `{}`.
    fn from_json(settings: &str) -> Result<Self, String> {
        let json = if settings.trim().is_empty() {
            "{}"
        } else {
            settings
        };
        let settings = serde_json::from_str(json).map_err(|e| format!("Invalid settings: {e}"))?;
        Self::init(settings)
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

/// Exports a [`Mode`] as the entry point of the wasm module.
///
/// Only expands on `wasm32` so the mode can still be built and tested natively.
#[macro_export]
macro_rules! export_mode {
    ($mode:ty) => {
        #[cfg(target_arch = "wasm32")]
        const _: () = {
            ::std::thread_local! {
                static MODE: ::core::cell::RefCell<::core::option::Option<$mode>> =
                    const { ::core::cell::RefCell::new(::core::option::Option::None) };
            }

            struct Export;

            impl $crate::bindings::Guest for Export {
                fn init(
                    settings: ::std::string::String,
                ) -> ::core::result::Result<(), ::std::string::String> {
                    let mode = <$mode as $crate::Mode>::from_json(&settings)?;
                    MODE.with(|m| *m.borrow_mut() = ::core::option::Option::Some(mode));
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
                        ::core::option::Option::Some(mode) => {
                            <$mode as $crate::Mode>::step(mode, &mut standings, event)
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
            }

    $crate::bindings::__export_mode!(Export with_types_in $crate::bindings);
        };
    };
}
