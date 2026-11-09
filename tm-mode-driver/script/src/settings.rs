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
