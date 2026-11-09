//! The settings of a script mode: the json of a match on top of the defaults the script declares.

use rhai::{Dynamic, Engine, Map};

/// The json settings of a match as a map, `{}` when empty.
pub(crate) fn parse(engine: &Engine, json: &str) -> Result<Map, String> {
    let json = if json.trim().is_empty() { "{}" } else { json };
    engine
        .parse_json(json, true)
        .map_err(|error| format!("Invalid settings: {error}"))
}

/// The settings a match plays with: every default of the script, replaced by what was given.
///
/// Settings the script does not declare are rejected, so typos do not silently
/// play the default. A given value has to be of the kind of its default, a
/// default of `()` takes anything and leaves the check to the script.
pub(crate) fn merge(defaults: &Map, given: Map) -> Result<Map, String> {
    merge_at("", defaults, given)
}

fn merge_at(path: &str, defaults: &Map, given: Map) -> Result<Map, String> {
    let mut settings = defaults.clone();
    for (name, value) in given {
        let at = format!("{path}{name}");
        let Some(default) = defaults.get(name.as_str()) else {
            return Err(format!("Invalid settings: unknown setting `{at}`"));
        };
        let value = if default.is_unit() {
            value
        } else if default.is_map() && value.is_map() {
            let default = default.clone().cast::<Map>();
            Dynamic::from_map(merge_at(&format!("{at}."), &default, value.cast::<Map>())?)
        } else if default.type_id() == value.type_id() {
            value
        } else {
            return Err(format!(
                "Invalid settings: `{at}` has to be {}",
                kind(default)
            ));
        };
        settings.insert(name, value);
    }
    Ok(settings)
}

/// What a setting has to be, for the message of a rejection.
fn kind(value: &Dynamic) -> &'static str {
    if value.is_int() {
        "a whole number"
    } else if value.is_bool() {
        "true or false"
    } else if value.is_string() {
        "a text"
    } else if value.is_array() {
        "a list"
    } else if value.is_map() {
        "an object"
    } else {
        "of another kind"
    }
}

/// The settings which differ between `old` and `new` but are not `reloadable`, in the order of their names.
pub(crate) fn fixed_changes(old: &Map, new: &Map, reloadable: &[String]) -> Vec<String> {
    old.iter()
        .filter(|(name, _)| !reloadable.iter().any(|r| r == name.as_str()))
        .filter(|(name, value)| !new.get(name.as_str()).is_some_and(|new| same(value, new)))
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Whether two settings are the same, lists and objects all the way down.
fn same(a: &Dynamic, b: &Dynamic) -> bool {
    if a.is_array() && b.is_array() {
        let (a, b) = (a.clone().into_array(), b.clone().into_array());
        return match (a, b) {
            (Ok(a), Ok(b)) => a.len() == b.len() && a.iter().zip(&b).all(|(a, b)| same(a, b)),
            _ => false,
        };
    }
    if a.is_map() && b.is_map() {
        let (a, b) = (a.clone().cast::<Map>(), b.clone().cast::<Map>());
        return a.len() == b.len()
            && a.iter()
                .all(|(name, a)| b.get(name.as_str()).is_some_and(|b| same(a, b)));
    }
    if a.is_unit() || b.is_unit() {
        return a.is_unit() && b.is_unit();
    }
    match (a.as_int(), b.as_int()) {
        (Ok(a), Ok(b)) => return a == b,
        (Err(_), Err(_)) => {}
        _ => return false,
    }
    match (a.as_bool(), b.as_bool()) {
        (Ok(a), Ok(b)) => return a == b,
        (Err(_), Err(_)) => {}
        _ => return false,
    }
    a.is_string()
        && b.is_string()
        && a.clone().into_immutable_string().ok() == b.clone().into_immutable_string().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(json: &str) -> Map {
        parse(&Engine::new_raw(), json).unwrap()
    }

    fn defaults() -> Map {
        map(
            r#"{"points": [10, 6], "rounds": 5, "name": "cup", "maps": null, "warmup": {"rounds": 0, "timeout": null}}"#,
        )
    }

    fn merged(json: &str) -> Result<Map, String> {
        merge(&defaults(), map(json))
    }

    #[test]
    fn what_is_not_given_plays_the_default() {
        let settings = merged(r#"{"rounds": 3, "warmup": {"rounds": 2}}"#).unwrap();

        assert_eq!(settings["rounds"].as_int(), Ok(3));
        assert!(same(&settings["points"], &defaults()["points"]));
        let warmup = settings["warmup"].clone().cast::<Map>();
        assert_eq!(warmup["rounds"].as_int(), Ok(2));
        assert!(warmup["timeout"].is_unit());
    }

    #[test]
    fn settings_the_script_does_not_declare_are_rejected() {
        assert_eq!(
            merged(r#"{"round": 3}"#).unwrap_err(),
            "Invalid settings: unknown setting `round`"
        );
        assert_eq!(
            merged(r#"{"warmup": {"round": 1}}"#).unwrap_err(),
            "Invalid settings: unknown setting `warmup.round`"
        );
    }

    #[test]
    fn a_setting_has_to_be_of_the_kind_of_its_default() {
        assert_eq!(
            merged(r#"{"rounds": "three"}"#).unwrap_err(),
            "Invalid settings: `rounds` has to be a whole number"
        );
        assert_eq!(
            merged(r#"{"points": 10}"#).unwrap_err(),
            "Invalid settings: `points` has to be a list"
        );
        assert_eq!(
            merged(r#"{"warmup": {"rounds": true}}"#).unwrap_err(),
            "Invalid settings: `warmup.rounds` has to be a whole number"
        );
        // A default of nothing leaves the check to the script.
        assert_eq!(merged(r#"{"maps": 3}"#).unwrap()["maps"].as_int(), Ok(3));
    }

    #[test]
    fn settings_which_are_no_object_are_rejected() {
        let engine = Engine::new_raw();

        assert!(parse(&engine, "[1, 2]").is_err());
        assert!(parse(&engine, r#"{"rounds": 1.5}"#).is_err());
        assert!(parse(&engine, "  ").unwrap().is_empty());
    }

    #[test]
    fn only_settings_which_differ_and_cannot_change_are_fixed_changes() {
        let old = merged("{}").unwrap();
        let reloadable = ["points".to_string(), "warmup".to_string()];
        let changes = |json: &str| fixed_changes(&old, &merged(json).unwrap(), &reloadable);

        assert_eq!(
            changes(r#"{"points": [1], "warmup": {"rounds": 1}}"#),
            [""; 0]
        );
        // Giving the default is no change.
        assert_eq!(changes(r#"{"rounds": 5, "name": "cup"}"#), [""; 0]);
        assert_eq!(changes(r#"{"rounds": 4, "maps": 2}"#), ["maps", "rounds"]);
    }
}
