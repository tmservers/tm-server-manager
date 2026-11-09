//! The modes built into the mode driver, by the name the config of a match selects
//! them with (`Driver::mode`). Every other mode is a script of a competition.

use tm_mode_driver_sdk::{ModeSource, Native};

/// The names of the built-in modes.
pub const NAMES: [&str; 5] = [
    "rounds",
    "time-attack",
    "reverse-cup",
    "knockout",
    "endurance",
];

/// The built-in mode with this name, none for any other name.
pub fn builtin(name: &str) -> Option<Box<dyn ModeSource + Send + Sync>> {
    Some(match name {
        "rounds" => Box::new(Native::<tm_mode_rounds::Rounds>::new()),
        "time-attack" => Box::new(Native::<tm_mode_time_attack::TimeAttack>::new()),
        "reverse-cup" => Box::new(Native::<tm_mode_reverse_cup::ReverseCup>::new()),
        "knockout" => Box::new(Native::<tm_mode_knockout::Knockout>::new()),
        "endurance" => Box::new(Native::<tm_mode_endurance::Endurance>::new()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_starts_its_mode_with_default_settings() {
        let shown: Vec<String> = NAMES
            .iter()
            .map(|name| {
                let mode = builtin(name).unwrap_or_else(|| panic!("{name} is not built in"));
                let instance = mode
                    .start("{}")
                    .expect("starting does not fault")
                    .unwrap_or_else(|error| panic!("{name} rejects its defaults: {error}"));
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
    fn other_names_are_no_built_in_modes() {
        assert!(builtin("Rounds").is_none());
        assert!(builtin("").is_none());
    }
}
