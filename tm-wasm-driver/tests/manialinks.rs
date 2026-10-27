//! Manialinks parse the standings the host publishes with `fromjson`, so every
//! manialink reading `Net_Driver_Standings` declares a struct member for every
//! field of the json.

use std::{collections::BTreeSet, path::PathBuf};

use tm_server_types::method::{DriverLabelTone, DriverStanding, DriverStandings, DriverStatus};

/// How the standings structs of a manialink declare their members.
const MEMBERS: [&str; 16] = [
    "Text title;",
    "Text info;",
    "Integer rounds_on_map;",
    "Integer maps_played;",
    "Text map_name;",
    "Integer map_count;",
    "K_Standing[] players;",
    "Text account_id;",
    "Text name;",
    "Integer rank;",
    "Text status;",
    "Text label;",
    "Text label_tone;",
    "Integer points;",
    "Integer map_points;",
    "Integer round_points;",
];

#[test]
fn members_cover_the_json() {
    let standings = DriverStandings {
        players: vec![DriverStanding {
            account_id: "a".into(),
            name: "a".into(),
            rank: 1,
            status: DriverStatus::Playing,
            label: String::new(),
            label_tone: DriverLabelTone::Neutral,
            points: 0,
            map_points: 0,
            round_points: 0,
        }],
        ..Default::default()
    };
    let json = serde_json::to_value(&standings).unwrap();

    let mut fields: BTreeSet<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    fields.extend(
        json["players"][0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str),
    );
    let members: BTreeSet<&str> = MEMBERS
        .iter()
        .map(|member| member.trim_end_matches(';').rsplit(' ').next().unwrap())
        .collect();
    assert_eq!(fields, members);
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
        if !manialink.contains("Net_Driver_Standings") {
            continue;
        }
        for member in MEMBERS {
            assert!(
                manialink.contains(member),
                "{member} is missing in {}",
                path.display()
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "No manialink reads the standings");
}
