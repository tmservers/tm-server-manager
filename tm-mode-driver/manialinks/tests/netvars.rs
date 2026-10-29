//! The default manialinks read nothing but the netvars of the driver modescript,
//! each with the type it has (`DRIVER_NETVARS`), so a typo or another type shows up
//! here rather than as an empty widget on a server.

use tm_server_types::method::DRIVER_NETVARS;

/// The netvars a manialink reads, by name and type.
fn netreads(manialink: &str) -> Vec<(&str, &str)> {
    manialink
        .lines()
        .filter_map(|line| line.trim().strip_prefix("declare netread "))
        .map(|declaration| {
            let declaration = declaration
                .strip_suffix(" for Teams[0];")
                .unwrap_or_else(|| panic!("{declaration} is not declared for Teams[0]"));
            let (kind, name) = declaration
                .rsplit_once(' ')
                .unwrap_or_else(|| panic!("{declaration} has no type"));
            (name, kind)
        })
        .collect()
}

#[test]
fn manialinks_read_the_netvars_of_the_driver() {
    for default in tm_mode_driver_manialinks::DEFAULTS {
        let reads = netreads(default.manialink);
        assert!(!reads.is_empty(), "{} reads no netvar", default.name);
        for (name, kind) in reads {
            let contract = DRIVER_NETVARS.iter().find(|(netvar, _)| *netvar == name);
            assert_eq!(
                contract.map(|(_, kind)| *kind),
                Some(kind),
                "{} reads {kind} {name}",
                default.name
            );
        }
    }
}

/// The netvars of every group, which change together with its serial.
fn group(name: &str) -> &'static str {
    match name {
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
        for (name, _) in &reads {
            let serial = format!("Net_Driver_{}Serial", group(name));
            assert!(
                reads.iter().any(|(read, _)| *read == serial),
                "{} reads {name} without watching {serial}",
                default.name
            );
        }
    }
}

#[test]
fn every_netvar_has_a_serial() {
    for (name, _) in DRIVER_NETVARS {
        let serial = format!("Net_Driver_{}Serial", group(name));
        assert!(
            DRIVER_NETVARS.iter().any(|(netvar, _)| *netvar == serial),
            "{name} has no serial"
        );
    }
}
