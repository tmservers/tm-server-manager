//! Manialinks parse what the host publishes with `fromjson`, so every manialink
//! reading a netvar of the host declares a struct member for every field of its json.

use std::{collections::BTreeSet, path::PathBuf};

use serde_json::Value;
use tm_server_types::method::{
    DriverEntry, DriverLabelTone, DriverRound, DriverStandings, DriverStatus, DriverUser,
    DriverUsers,
};

/// How the structs of a manialink declare the members of an entry.
const ENTRY: [&str; 7] = [
    "Text account_id;",
    "Integer position;",
    "Integer score;",
    "Integer time;",
    "Text status;",
    "Text label;",
    "Text label_tone;",
];

/// The netvars of the host and how the structs of a manialink declare their members.
fn netvars() -> [(&'static str, Vec<&'static str>, Value); 3] {
    let entry = DriverEntry {
        account_id: "a".into(),
        position: 1,
        score: 0,
        time: -1,
        status: DriverStatus::Playing,
        label: String::new(),
        label_tone: DriverLabelTone::Neutral,
    };
    let standings = DriverStandings {
        entries: vec![entry.clone()],
        ..Default::default()
    };
    let round = DriverRound {
        number: 1,
        entries: vec![entry],
    };
    let users = DriverUsers {
        users: vec![DriverUser::default()],
    };

    [
        (
            "Net_Driver_Standings",
            [
                "Text title;",
                "Text info;",
                "Integer rounds_on_map;",
                "Integer maps_played;",
                "Text map_name;",
                "Integer map_count;",
                "Boolean paused;",
                "K_Entry[] entries;",
            ]
            .into_iter()
            .chain(ENTRY)
            .collect(),
            serde_json::to_value(standings).unwrap(),
        ),
        (
            "Net_Driver_Round",
            ["Integer number;", "K_Entry[] entries;"]
                .into_iter()
                .chain(ENTRY)
                .collect(),
            serde_json::to_value(round).unwrap(),
        ),
        (
            "Net_Driver_Users",
            vec![
                "K_User[] users;",
                "Text account_id;",
                "Text name;",
                "Text club_tag;",
                "Text zone;",
            ],
            serde_json::to_value(users).unwrap(),
        ),
    ]
}

/// The keys of an object and of the objects in its arrays.
fn keys(json: &Value, keys: &mut BTreeSet<String>) {
    match json {
        Value::Object(object) => {
            for (key, value) in object {
                keys.insert(key.clone());
                self::keys(value, keys);
            }
        }
        Value::Array(array) => {
            for value in array {
                self::keys(value, keys);
            }
        }
        _ => {}
    }
}

#[test]
fn members_cover_the_json() {
    for (netvar, members, json) in netvars() {
        let mut fields = BTreeSet::new();
        keys(&json, &mut fields);
        let members: BTreeSet<String> = members
            .iter()
            .map(|member| {
                member
                    .trim_end_matches(';')
                    .rsplit(' ')
                    .next()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(fields, members, "{netvar}");
    }
}

#[test]
fn manialinks_declare_every_member() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tm-driver-modes/manialinks");
    let mut checked = 0;
    for entry in std::fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "xml") {
            continue;
        }
        let manialink = std::fs::read_to_string(&path).unwrap();
        for (netvar, members, _) in netvars() {
            if !manialink.contains(&format!("Text {netvar} ")) {
                continue;
            }
            for member in members {
                assert!(
                    manialink.contains(member),
                    "{member} of {netvar} is missing in {}",
                    path.display()
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "No manialink reads the netvars of the host");
}
