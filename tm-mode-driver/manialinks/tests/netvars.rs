//! The default manialinks read nothing but the netvars of the driver modescript,
//! each with the type it has (`DRIVER_NETVARS`), so a typo or another type shows up
//! here rather than as an empty widget on a server.

use tm_server_types::method::{DRIVER_NETVARS, DRIVER_PLAYER_NETVARS};

/// The netvars a manialink reads, by name and type, and whether they are of the player.
fn netreads(manialink: &str) -> Vec<(&str, &str, bool)> {
    manialink
        .lines()
        .filter_map(|line| line.trim().strip_prefix("declare netread "))
        .map(|declaration| {
            let (declaration, player) = match declaration.strip_suffix(" for UI;") {
                Some(declaration) => (declaration, true),
                None => (
                    declaration
                        .strip_suffix(" for Teams[0];")
                        .unwrap_or_else(|| panic!("{declaration} is not declared for Teams[0]")),
                    false,
                ),
            };
            let (kind, name) = declaration
                .rsplit_once(' ')
                .unwrap_or_else(|| panic!("{declaration} has no type"));
            (name, kind, player)
        })
        .collect()
}

#[test]
fn manialinks_read_the_netvars_of_the_driver() {
    for default in tm_mode_driver_manialinks::DEFAULTS {
        let reads = netreads(default.manialink);
        assert!(!reads.is_empty(), "{} reads no netvar", default.name);
        for (name, kind, player) in reads {
            let netvars: &[(&str, &str)] = if player {
                &DRIVER_PLAYER_NETVARS
            } else {
                &DRIVER_NETVARS
            };
            let contract = netvars.iter().find(|(netvar, _)| *netvar == name);
            assert_eq!(
                contract.map(|(_, kind)| *kind),
                Some(kind),
                "{} reads {kind} {name}",
                default.name
            );
        }
    }
}

/// The netvars of every group, which change together with its serial. The index and
/// team of a player change with the users and teams.
fn group(name: &str) -> &'static str {
    match name {
        "Net_Driver_MyUser" => "Users",
        "Net_Driver_MyTeam" => "Teams",
        _ if name.starts_with("Net_Driver_MyData") => "MyData",
        _ if name.starts_with("Net_Driver_Data") => "Data",
        "Net_Driver_UserTeams" => "Teams",
        _ if name.starts_with("Net_Driver_Team") => "Teams",
        _ if name.starts_with("Net_Driver_User") => "Users",
        _ if name.starts_with("Net_Driver_Round") && name != "Net_Driver_RoundsOnMap" => "Round",
        _ if name.starts_with("Net_Driver_State") => "State",
        _ => "Standings",
    }
}

#[test]
fn manialinks_watch_the_serials_of_what_they_read() {
    for default in tm_mode_driver_manialinks::DEFAULTS {
        let reads = netreads(default.manialink);
        for (name, _, _) in &reads {
            let serial = format!("Net_Driver_{}Serial", group(name));
            assert!(
                reads.iter().any(|(read, _, _)| *read == serial),
                "{} reads {name} without watching {serial}",
                default.name
            );
        }
    }
}

#[test]
fn every_netvar_has_a_serial() {
    for (name, _) in DRIVER_NETVARS.into_iter().chain(DRIVER_PLAYER_NETVARS) {
        let serial = format!("Net_Driver_{}Serial", group(name));
        assert!(
            DRIVER_NETVARS
                .into_iter()
                .chain(DRIVER_PLAYER_NETVARS)
                .any(|(netvar, _)| netvar == serial),
            "{name} has no serial"
        );
    }
}

/// Every manialink a config can select is one of the defaults.
#[test]
fn every_built_in_manialink_of_a_config_exists() {
    for manialink in tm_server_types::config::BuiltInManialink::ALL {
        assert!(
            tm_mode_driver_manialinks::default(manialink.name()).is_some(),
            "{} is no default manialink",
            manialink.name()
        );
    }
}

/// What a default manialink sends is an event of its manifest with as many arguments,
/// and the keys it reads are the data keys of the manifest.
#[test]
fn manialinks_send_the_events_of_their_manifest() {
    for manialink in tm_server_types::config::BuiltInManialink::ALL {
        let xml = tm_mode_driver_manialinks::default(manialink.name()).unwrap();
        let manifest = manialink.manifest();
        let mut rest = xml;
        while let Some(start) = rest.find("SendCustomEvent(\"") {
            rest = &rest[start + "SendCustomEvent(\"".len()..];
            let (name, after) = rest.split_once('"').unwrap();
            let args = after[..after.find(']').unwrap()].matches('"').count() / 2;
            let (_, event) = manifest.event(name).unwrap_or_else(|| {
                panic!(
                    "{} sends {name}, which it does not declare",
                    manialink.name()
                )
            });
            assert_eq!(event.args.len(), args, "{} sends {name}", manialink.name());
        }
        for key in &manifest.data_keys {
            assert!(
                xml.contains(&format!("[\"{key}\"]")),
                "{} declares {key} without reading it",
                manialink.name()
            );
        }
        let reads_data = xml.contains("Net_Driver_Data ") || xml.contains("Net_Driver_MyData ");
        assert_eq!(
            reads_data,
            !manifest.data_keys.is_empty(),
            "{}",
            manialink.name()
        );
    }
}
