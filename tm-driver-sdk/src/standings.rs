//! Standings for the first-party standings manialink.
//!
//! The modescript keeps no points, so modes publish their standings through
//! `Net_Driver_State`. The manialink `tm-driver-modes/manialinks/standings.xml`
//! parses exactly this json, so field names must not change.

use serde::Serialize;

use crate::Command;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Standings {
    /// Shown above the standings, e.g. the name of the mode.
    pub title: String,
    /// Short progress text, e.g. `2 / 5 rounds`.
    pub info: String,
    /// Ordered from first to last.
    pub players: Vec<Standing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Standing {
    pub account_id: String,
    pub name: String,
    pub points: i32,
    /// Points of the last round. 0 hides them.
    pub round_points: i32,
}

impl Standings {
    /// Shows the standings to everyone.
    pub fn command(&self) -> Command {
        Command::SetNetState(serde_json::to_string(self).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The structs of the manialink parse this json with `fromjson`.
    #[test]
    fn manialink_knows_every_field() {
        let manialink = include_str!("../../tm-driver-modes/manialinks/standings.xml");
        for member in [
            "Text title;",
            "Text info;",
            "K_Standing[] players;",
            "Text account_id;",
            "Text name;",
            "Integer points;",
            "Integer round_points;",
        ] {
            assert!(
                manialink.contains(member),
                "{member} is missing in standings.xml"
            );
        }
    }

    /// The manialink depends on these names.
    #[test]
    fn json_shape() {
        let standings = Standings {
            title: "Rounds".into(),
            info: "1 / 5 rounds".into(),
            players: vec![Standing {
                account_id: "a".into(),
                name: "$f00kistz".into(),
                points: 10,
                round_points: 10,
            }],
        };
        assert_eq!(
            standings.command(),
            Command::SetNetState(
                r#"{"title":"Rounds","info":"1 / 5 rounds","players":[{"account_id":"a","name":"$f00kistz","points":10,"round_points":10}]}"#
                    .into()
            )
        );
    }
}
