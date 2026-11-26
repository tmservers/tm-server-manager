//! What players do in manialinks: the events a manialink sends and where they go.
//!
//! A manialink sends text only, `SendCustomEvent(Type, Data)` or a `scriptaction`. Its
//! manifest declares every event with typed arguments, the bridge parses them at the
//! boundary to the server and drops what does not fit, so nothing behind it sees text a
//! player typed where a number belongs.

/// The events a manialink sends and the data keys it reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct UiManifest {
    pub events: Vec<UiEventSpec>,
    /// The keys of `Net_Driver_Data` and `Net_Driver_MyData` it reads.
    pub data_keys: Vec<String>,
}

impl UiManifest {
    /// The event of this name and its index, which records and the manager refer to it by.
    pub fn event(&self, name: &str) -> Option<(u32, &UiEventSpec)> {
        self.events
            .iter()
            .enumerate()
            .find(|(_, event)| event.name == name)
            .map(|(index, event)| (index as u32, event))
    }
}

/// The most events and data keys a manifest declares, choices an argument offers and
/// bytes a name or a text argument has.
pub const MAX_UI_EVENTS: usize = 64;
pub const MAX_UI_DATA_KEYS: usize = 64;
pub const MAX_UI_CHOICES: usize = 64;
pub const MAX_UI_NAME: usize = 64;
pub const MAX_UI_TEXT: u32 = 1024;

impl UiManifest {
    /// Whether the manifest can be kept: bounded, every name once and every argument
    /// one a player can send.
    pub fn check(&self) -> Result<(), String> {
        if self.events.len() > MAX_UI_EVENTS {
            return Err(format!("A manialink sends at most {MAX_UI_EVENTS} events."));
        }
        if self.data_keys.len() > MAX_UI_DATA_KEYS {
            return Err(format!(
                "A manialink reads at most {MAX_UI_DATA_KEYS} data keys."
            ));
        }
        let name = |kind: &str, name: &str| {
            if name.is_empty() || name.len() > MAX_UI_NAME {
                return Err(format!(
                    "The name of {kind} has between 1 and {MAX_UI_NAME} bytes: \"{name}\""
                ));
            }
            Ok(())
        };
        for (index, event) in self.events.iter().enumerate() {
            name("an event", &event.name)?;
            if self.events[..index].iter().any(|e| e.name == event.name) {
                return Err(format!("The event {} is declared twice.", event.name));
            }
            if event.args.len() > MAX_UI_ARGS {
                return Err(format!(
                    "{} has {} arguments, at most {MAX_UI_ARGS} fit.",
                    event.name,
                    event.args.len()
                ));
            }
            for arg in &event.args {
                match arg {
                    UiArg::Int(range) if range.min > range.max => {
                        return Err(format!(
                            "{} takes a number between {} and {}, which is none.",
                            event.name, range.min, range.max
                        ));
                    }
                    UiArg::Text(max) if *max > MAX_UI_TEXT => {
                        return Err(format!(
                            "{} takes a text of at most {MAX_UI_TEXT} bytes, not {max}.",
                            event.name
                        ));
                    }
                    UiArg::Choice(choices)
                        if choices.is_empty() || choices.len() > MAX_UI_CHOICES =>
                    {
                        return Err(format!(
                            "{} offers between 1 and {MAX_UI_CHOICES} choices.",
                            event.name
                        ));
                    }
                    _ => {}
                }
            }
        }
        for (index, key) in self.data_keys.iter().enumerate() {
            name("a data key", key)?;
            if self.data_keys[..index].contains(key) {
                return Err(format!("The data key {key} is declared twice."));
            }
        }
        Ok(())
    }
}

/// An event a manialink sends.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct UiEventSpec {
    /// The `CustomEventType`.
    pub name: String,
    pub route: UiRoute,
    pub args: Vec<UiArg>,
}

/// Where an event goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum UiRoute {
    /// To the mode, as an input of the driver: it may decide the match, the bridge
    /// decides without waiting and the manager replays it.
    Mode,
    /// To the manager, e.g. votes or feedback. It never changes the scores.
    Manager,
}

/// What an argument of an event has to be.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum UiArg {
    /// A whole number between the bounds, both included. Modes play without floats, so
    /// e.g. a slider sends tenths of a second.
    Int(UiIntRange),
    /// `1` or `0`, `True` or `False`.
    Bool,
    /// Text of at most this many bytes.
    Text(u32),
    /// One of these, parsed to its index.
    Choice(Vec<String>),
    /// An account of a player of the match.
    User,
    /// A team of the match by its id.
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct UiIntRange {
    pub min: i32,
    pub max: i32,
}

/// An argument of an event as it was parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum UiValue {
    Int(i32),
    Bool(bool),
    Text(String),
    Choice(u32),
    /// The account of the player.
    User(String),
    Team(u32),
}

/// The most arguments an event can carry.
pub const MAX_UI_ARGS: usize = 16;

impl UiEventSpec {
    /// Parses the text a manialink sent. `player` tells whether an account plays in the
    /// match, `team` whether a team does.
    pub fn parse(
        &self,
        data: &[String],
        player: impl Fn(&str) -> bool,
        team: impl Fn(u32) -> bool,
    ) -> Result<Vec<UiValue>, String> {
        if data.len() != self.args.len() || data.len() > MAX_UI_ARGS {
            return Err(format!(
                "{} takes {} arguments, not {}",
                self.name,
                self.args.len(),
                data.len()
            ));
        }
        self.args
            .iter()
            .zip(data)
            .enumerate()
            .map(|(index, (arg, text))| {
                parse(arg, text, &player, &team).ok_or_else(|| {
                    format!(
                        "Argument {} of {} does not fit: {text}",
                        index + 1,
                        self.name
                    )
                })
            })
            .collect()
    }

    /// Whether parsed values fit the event, e.g. when the manager gets them.
    pub fn fits(&self, values: &[UiValue]) -> bool {
        values.len() == self.args.len()
            && self
                .args
                .iter()
                .zip(values)
                .all(|(arg, value)| match (arg, value) {
                    (UiArg::Int(range), UiValue::Int(n)) => (range.min..=range.max).contains(n),
                    (UiArg::Bool, UiValue::Bool(_)) => true,
                    (UiArg::Text(max), UiValue::Text(text)) => text.len() <= *max as usize,
                    (UiArg::Choice(choices), UiValue::Choice(index)) => {
                        (*index as usize) < choices.len()
                    }
                    (UiArg::User, UiValue::User(_)) | (UiArg::Team, UiValue::Team(_)) => true,
                    _ => false,
                })
    }
}

fn parse(
    arg: &UiArg,
    text: &str,
    player: &impl Fn(&str) -> bool,
    team: &impl Fn(u32) -> bool,
) -> Option<UiValue> {
    match arg {
        UiArg::Int(range) => {
            let value: i32 = text.trim().parse().ok()?;
            (range.min..=range.max)
                .contains(&value)
                .then_some(UiValue::Int(value))
        }
        UiArg::Bool => match text.trim() {
            "1" | "True" | "true" => Some(UiValue::Bool(true)),
            "0" | "False" | "false" => Some(UiValue::Bool(false)),
            _ => None,
        },
        UiArg::Text(max) => (text.len() <= *max as usize).then(|| UiValue::Text(text.into())),
        UiArg::Choice(choices) => choices
            .iter()
            .position(|choice| choice == text)
            .map(|index| UiValue::Choice(index as u32)),
        UiArg::User => player(text).then(|| UiValue::User(text.into())),
        UiArg::Team => {
            let id: u32 = text.trim().parse().ok()?;
            team(id).then_some(UiValue::Team(id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vote() -> UiEventSpec {
        UiEventSpec {
            name: "vote".into(),
            route: UiRoute::Manager,
            args: vec![
                UiArg::Int(UiIntRange { min: 1, max: 5 }),
                UiArg::Bool,
                UiArg::Text(8),
                UiArg::Choice(vec!["map".into(), "player".into()]),
                UiArg::User,
                UiArg::Team,
            ],
        }
    }

    fn parse(data: &[&str]) -> Result<Vec<UiValue>, String> {
        let data: Vec<String> = data.iter().map(|d| d.to_string()).collect();
        vote().parse(&data, |account| account == "a", |team| team == 7)
    }

    #[test]
    fn arguments_parse_to_their_types() {
        let values = parse(&["3", "True", "gg", "player", "a", "7"]).unwrap();
        assert_eq!(
            values,
            [
                UiValue::Int(3),
                UiValue::Bool(true),
                UiValue::Text("gg".into()),
                UiValue::Choice(1),
                UiValue::User("a".into()),
                UiValue::Team(7),
            ]
        );
        assert!(vote().fits(&values));
    }

    #[test]
    fn arguments_which_do_not_fit_are_refused() {
        let wrong = [
            ["6", "1", "", "map", "a", "7"],
            ["3", "yes", "", "map", "a", "7"],
            ["3", "1", "too long!", "map", "a", "7"],
            ["3", "1", "", "track", "a", "7"],
            ["3", "1", "", "map", "b", "7"],
            ["3", "1", "", "map", "a", "8"],
        ];
        for (index, data) in wrong.iter().enumerate() {
            let refused = parse(data).unwrap_err();
            assert!(
                refused.contains(&format!("Argument {}", index + 1)),
                "{refused}"
            );
        }
        assert_eq!(parse(&["3"]).unwrap_err(), "vote takes 6 arguments, not 1");
    }

    #[test]
    fn values_of_another_type_do_not_fit() {
        let spec = vote();
        let mut values = parse(&["3", "0", "", "map", "a", "7"]).unwrap();
        values[0] = UiValue::Int(9);
        assert!(!spec.fits(&values));
        values[0] = UiValue::Bool(true);
        assert!(!spec.fits(&values));
        assert!(!spec.fits(&[]));
    }

    #[test]
    fn manifests_are_bounded_and_name_everything_once() {
        let manifest = |events: Vec<UiEventSpec>, data_keys: &[&str]| UiManifest {
            events,
            data_keys: data_keys.iter().map(|k| k.to_string()).collect(),
        };
        let event = |name: &str, args: Vec<UiArg>| UiEventSpec {
            name: name.into(),
            route: UiRoute::Manager,
            args,
        };
        assert_eq!(manifest(vec![vote()], &["ready"]).check(), Ok(()));
        let refused = [
            (
                manifest(vec![vote(), vote()], &[]),
                "The event vote is declared twice.",
            ),
            (
                manifest(vec![], &["a", "a"]),
                "The data key a is declared twice.",
            ),
            (
                manifest(vec![event("", Vec::new())], &[]),
                "The name of an event has between 1 and 64 bytes: \"\"",
            ),
            (
                manifest(vec![], &[&"k".repeat(65)]),
                "The name of a data key has between 1 and 64 bytes",
            ),
            (
                manifest(
                    vec![event("n", vec![UiArg::Int(UiIntRange { min: 2, max: 1 })])],
                    &[],
                ),
                "n takes a number between 2 and 1, which is none.",
            ),
            (
                manifest(vec![event("t", vec![UiArg::Text(MAX_UI_TEXT + 1)])], &[]),
                "t takes a text of at most 1024 bytes, not 1025.",
            ),
            (
                manifest(vec![event("c", vec![UiArg::Choice(Vec::new())])], &[]),
                "c offers between 1 and 64 choices.",
            ),
            (
                manifest(vec![event("many", vec![UiArg::Bool; MAX_UI_ARGS + 1])], &[]),
                "many has 17 arguments, at most 16 fit.",
            ),
        ];
        for (manifest, error) in refused {
            let refused = manifest.check().unwrap_err();
            assert!(refused.starts_with(error), "{refused}");
        }
        let events = (0..=MAX_UI_EVENTS)
            .map(|i| event(&i.to_string(), Vec::new()))
            .collect();
        assert!(manifest(events, &[]).check().is_err());
        let keys: Vec<String> = (0..=MAX_UI_DATA_KEYS).map(|i| i.to_string()).collect();
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        assert!(manifest(vec![], &keys).check().is_err());
        // The bounds themselves fit.
        let events = (0..MAX_UI_EVENTS)
            .map(|i| event(&i.to_string(), vec![UiArg::Bool; MAX_UI_ARGS]))
            .collect();
        assert_eq!(manifest(events, &keys[..MAX_UI_DATA_KEYS]).check(), Ok(()));
    }

    #[test]
    fn events_are_found_by_name_with_their_index() {
        let manifest = UiManifest {
            events: vec![
                vote(),
                UiEventSpec {
                    name: "ready".into(),
                    route: UiRoute::Mode,
                    args: Vec::new(),
                },
            ],
            data_keys: Vec::new(),
        };
        assert_eq!(manifest.event("ready").map(|(index, _)| index), Some(1));
        assert!(manifest.event("nothing").is_none());
    }
}
