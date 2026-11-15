//! The modes built into the mode driver, by the version the config of a match selects
//! them with (`Driver::mode`).
//!
//! A released version never changes how it plays, the snapshots of its simulations in
//! `released.txt` hold it to that. A change of rules is a new version next to it.

pub use tm_server_types::config::BuiltInMode;
use tm_mode_driver_sdk::{ModeSource, Native};

#[cfg(test)]
mod released;

/// The built-in mode of this version.
pub fn builtin(mode: BuiltInMode) -> Box<dyn ModeSource + Send + Sync> {
    match mode {
        BuiltInMode::RoundsV1 => Box::new(Native::<tm_mode_rounds::v1::Rounds>::new()),
        BuiltInMode::TimeAttackV1 => {
            Box::new(Native::<tm_mode_time_attack::v1::TimeAttack>::new())
        }
        BuiltInMode::ReverseCupV1 => {
            Box::new(Native::<tm_mode_reverse_cup::v1::ReverseCup>::new())
        }
        BuiltInMode::KnockoutV1 => Box::new(Native::<tm_mode_knockout::v1::Knockout>::new()),
        BuiltInMode::EnduranceV1 => Box::new(Native::<tm_mode_endurance::v1::Endurance>::new()),
    }
}

/// The names of the modes this driver plays, e.g. `rounds-v1`, the ones superseded included.
/// A bridge reports them, so the manager only gives it matches it can play.
pub fn supported() -> Vec<String> {
    BuiltInMode::ALL.iter().map(|mode| mode.name()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_starts_with_default_settings() {
        let shown: Vec<String> = BuiltInMode::ALL
            .iter()
            .map(|mode| {
                let instance = builtin(*mode)
                    .start("{}")
                    .expect("starting does not fault")
                    .unwrap_or_else(|error| panic!("{mode:?} rejects its defaults: {error}"));
                instance.name()
            })
            .collect();

        assert_eq!(
            shown,
            [
                "Rounds",
                "Time Attack",
                "Reverse Cup",
                "Knockout",
                "Endurance"
            ]
        );
    }

    #[test]
    fn the_names_bridges_report_parse_back() {
        let supported = supported();
        assert_eq!(supported.len(), BuiltInMode::ALL.len());
        for (name, mode) in supported.iter().zip(BuiltInMode::ALL) {
            assert_eq!(BuiltInMode::named(name), Some(*mode));
        }
    }
}
