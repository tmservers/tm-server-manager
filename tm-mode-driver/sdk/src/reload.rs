//! Settings that change during a match.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::Mode;

/// A mode and its settings with every default filled in, to tell what new settings
/// change. The export of a mode keeps one for the whole match.
pub struct Configured<M> {
    pub mode: M,
    pub(crate) settings: Value,
}

impl<M: Mode> Configured<M> {
    /// The mode with the json settings of the match, `{}` when empty.
    pub fn new(json: &str) -> Result<Self, String> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let settings: M::Settings =
            serde_json::from_str(json).map_err(|e| format!("Invalid settings: {e}"))?;
        let value =
            serde_json::to_value(&settings).map_err(|e| format!("Invalid settings: {e}"))?;
        Ok(Self {
            mode: M::init(settings)?,
            settings: value,
        })
    }

    /// The mode with new settings during the match. Rejects settings that change
    /// anything [`Mode::RELOADABLE`] does not list.
    pub fn change(&self, json: &str) -> Result<Self, String> {
        let next = Self::new(json)?;
        match fixed_changes(&self.settings, &next.settings, M::RELOADABLE).as_slice() {
            [] => Ok(next),
            fixed => Err(format!("{} cannot change during a match", fixed.join(", "))),
        }
    }
}

/// The settings that differ between `old` and `new` but may not change.
fn fixed_changes(old: &Value, new: &Value, reloadable: &[&str]) -> Vec<String> {
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            let names: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
            names
                .into_iter()
                .filter(|name| !reloadable.contains(&name.as_str()))
                .filter(|name| old.get(*name) != new.get(*name))
                .cloned()
                .collect()
        }
        // Settings that are no json object change as a whole.
        _ if old != new => vec!["the settings".into()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::{Command, Event, Standings};

    #[derive(Debug, Default, Deserialize, Serialize)]
    #[serde(default, deny_unknown_fields)]
    struct Settings {
        points: Vec<i32>,
        lives: i32,
        rounds: u32,
    }

    struct Test;

    impl Mode for Test {
        const NAME: &'static str = "Test";
        const RELOADABLE: &'static [&'static str] = &["points"];

        type Settings = Settings;

        fn init(settings: Settings) -> Result<Self, String> {
            if settings.lives < 0 {
                return Err("Invalid settings: lives must not be negative".into());
            }
            Ok(Self)
        }

        fn step(&self, _: &mut Standings, _: Event) -> Vec<Command> {
            Vec::new()
        }
    }

    fn change(old: &str, new: &str) -> Result<(), String> {
        Configured::<Test>::new(old)?.change(new).map(|_| ())
    }

    #[test]
    fn only_reloadable_settings_change() {
        assert_eq!(
            change(r#"{"lives": 2}"#, r#"{"lives": 2, "points": [5]}"#),
            Ok(())
        );
        assert_eq!(
            change(r#"{"lives": 2}"#, r#"{"lives": 3, "points": [5]}"#),
            Err("lives cannot change during a match".into())
        );
        assert_eq!(
            change("{}", r#"{"lives": 3, "rounds": 4}"#),
            Err("lives, rounds cannot change during a match".into())
        );
    }

    #[test]
    fn settings_are_compared_with_their_defaults() {
        // Leaving out a setting is the same as giving its default.
        assert_eq!(change(r#"{"lives": 0}"#, "{}"), Ok(()));
        assert_eq!(change("", r#"{"rounds": 0}"#), Ok(()));
    }

    #[test]
    fn new_settings_are_validated_like_the_first_ones() {
        assert!(change("{}", r#"{"point": [5]}"#).is_err());
        assert_eq!(
            change("{}", r#"{"lives": -1}"#),
            Err("Invalid settings: lives must not be negative".into())
        );
    }
}
