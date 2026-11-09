//! The default manialinks of driver matches.
//!
//! Manialinks read nothing but the netvars the driver modescript writes from what
//! the driver host publishes, so every manialink works with every mode. Modes bring
//! none: the config of a match selects manialinks by name, these defaults or ones a
//! tournament supplies, e.g. with a style of its own. A name ending with
//! `scores-table` makes a manialink the scores table of the game.

/// A manialink by the name a config selects it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manialink {
    pub name: &'static str,
    pub manialink: &'static str,
}

/// Every default manialink:
///
/// - `standings`: the top 10 of the match with the points of the round.
/// - `my-standing`: where the local player stands.
/// - `scores-table`: replaces the scores table of the game.
/// - `live-standings`: the round instead of the match, live while it runs, e.g. for
///   the best times of a time attack map or an endurance race.
pub const DEFAULTS: [Manialink; 4] = [
    Manialink {
        name: "standings",
        manialink: include_str!("../manialinks/standings.xml"),
    },
    Manialink {
        name: "my-standing",
        manialink: include_str!("../manialinks/my-standing.xml"),
    },
    Manialink {
        name: "scores-table",
        manialink: include_str!("../manialinks/scores-table.xml"),
    },
    Manialink {
        name: "live-standings",
        manialink: include_str!("../manialinks/live-standings.xml"),
    },
];

/// The default manialink with this name.
pub fn default(name: &str) -> Option<&'static str> {
    DEFAULTS
        .iter()
        .find(|default| default.name == name)
        .map(|default| default.manialink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_found_by_name() {
        assert!(default("standings").is_some_and(|m| m.contains("Driver_Standings")));
        assert!(default("live-standings").is_some_and(|m| m.contains("Driver_LiveStandings")));
        assert_eq!(default("nothing"), None);
    }
}
