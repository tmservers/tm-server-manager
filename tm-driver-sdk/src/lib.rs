//! Write wasm modes for the driver of tm-server-manager.
//!
//! Implement [`Mode`] and export it with [`export_mode!`]. Build the crate as a
//! `cdylib` for `wasm32-unknown-unknown`. The driver host turns the resulting
//! core module into a component when it loads it.
//!
//! Modes run without WASI. Everything they know comes from [`Event`]s, so a
//! mode must not depend on wall clock time or randomness to stay replayable.

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

pub use bindings::tmservers::driver::host::LogLevel;
pub use bindings::tmservers::driver::types::{
    CanSpawn, Command, Event, FinishTimeout, Layer, MapStart, MatchStart, Player, PlayerJoin,
    PlayerLeave, PlayerPoints, RaceResult, RoundEnd, RoundStart,
};

/// A game mode. One instance lives for the whole match.
pub trait Mode: Sized + 'static {
    /// Creates the mode from the json settings of the match.
    fn init(settings: &str) -> Result<Self, String>;
    /// Handles an event and returns the commands for the server in order.
    fn step(&mut self, event: Event) -> Vec<Command>;
    /// Serializes the whole state including the settings.
    fn snapshot(&self) -> Vec<u8>;
    /// Recreates the mode from [`Mode::snapshot`].
    fn restore(snapshot: &[u8]) -> Result<Self, String>;
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
                    let mode = <$mode as $crate::Mode>::init(&settings)?;
                    MODE.with(|m| *m.borrow_mut() = ::core::option::Option::Some(mode));
                    ::core::result::Result::Ok(())
                }

                fn step(event: $crate::Event) -> ::std::vec::Vec<$crate::Command> {
                    MODE.with(|m| match m.borrow_mut().as_mut() {
                        ::core::option::Option::Some(mode) => {
                            <$mode as $crate::Mode>::step(mode, event)
                        }
                        ::core::option::Option::None => {
                            $crate::log($crate::LogLevel::Error, "step was called before init");
                            ::std::vec::Vec::new()
                        }
                    })
                }

                fn snapshot() -> ::std::vec::Vec<u8> {
                    MODE.with(|m| {
                        m.borrow()
                            .as_ref()
                            .map(<$mode as $crate::Mode>::snapshot)
                            .unwrap_or_default()
                    })
                }

                fn restore(
                    snapshot: ::std::vec::Vec<u8>,
                ) -> ::core::result::Result<(), ::std::string::String> {
                    let mode = <$mode as $crate::Mode>::restore(&snapshot)?;
                    MODE.with(|m| *m.borrow_mut() = ::core::option::Option::Some(mode));
                    ::core::result::Result::Ok(())
                }
            }

    $crate::bindings::__export_mode!(Export with_types_in $crate::bindings);
        };
    };
}
