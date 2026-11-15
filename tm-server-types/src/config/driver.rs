use std::collections::BTreeMap;

use super::{DriverMode, DriverUi};

/// The generic driver modescript. The rules of the mode run in the mode driver next to the
/// server, a built-in mode or a script of the competition.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Driver {
    /// The mode and its version. Matches keep it, new configs have to use the latest version.
    pub mode: DriverMode,
    /// Json settings passed to the mode when it is initialized.
    pub settings: String,
    /// Milliseconds the modescript waits for a decision before showing a waiting
    /// message and asking again.
    pub decision_timeout: u32,
    /// What players see: the manialinks the server shows next to the mode, where and
    /// to whom, and the UI modules of the game. The modescript keeps no points, so
    /// the manialinks show what the host publishes.
    pub ui: DriverUi,
}

impl Driver {
    pub const SCRIPT_NAME: &str = "Modes/Trackmania/TM_Driver";
    pub const SCRIPT: &str = include_str!("../../external_modes/TM_Driver.Script.txt");

    pub fn into_xml(&self) -> String {
        format!(
            r#"
        <setting name="S_DriverDecisionTimeout" value="{}" type="integer"/>
            "#,
            self.decision_timeout_setting(),
        )
    }

    pub(super) fn get_xml_map(&self) -> BTreeMap<String, dxr::Value> {
        let mut map = BTreeMap::new();

        map.insert(
            "S_DriverDecisionTimeout".into(),
            dxr::Value::Integer(self.decision_timeout_setting()),
        );

        map
    }

    fn decision_timeout_setting(&self) -> i32 {
        self.decision_timeout.min(i32::MAX as u32) as i32
    }
}

impl Default for Driver {
    fn default() -> Self {
        Self {
            mode: DriverMode::BuiltIn(super::BuiltInMode::RoundsV1),
            settings: "{}".into(),
            decision_timeout: 10_000,
            ui: DriverUi::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::DriverHold, method::DriverCommand};

    /// The modescript and the Rust types have to agree on every name.
    #[test]
    fn script_knows_every_callback_and_method() {
        for name in std::iter::once(DriverHold::CALLBACK).chain(DriverCommand::MODESCRIPT_METHODS) {
            assert!(
                Driver::SCRIPT.contains(&format!("\"{name}\"")),
                "{name} is missing in TM_Driver.Script.txt"
            );
        }
    }

    /// Every kind of layer becomes the layer type of the game it names.
    #[test]
    fn script_knows_layer_kinds() {
        use crate::method::DriverLayerKind;
        for kind in DriverLayerKind::ALL {
            assert!(
                Driver::SCRIPT.contains(&format!("\"{}\"", kind.code())),
                "{} is missing in TM_Driver.Script.txt",
                kind.code()
            );
            if kind != DriverLayerKind::Normal {
                assert!(
                    Driver::SCRIPT.contains(&format!("CUILayer::EUILayerType::{kind:?}")),
                    "the layer type {kind:?} is missing in TM_Driver.Script.txt"
                );
            }
        }
    }

    /// The audiences, anchors, labels and visibilities of the config are the ones the
    /// script checks.
    #[cfg(feature = "serde")]
    #[test]
    fn script_knows_audiences_and_markers() {
        use crate::config::{
            Audience, BuiltInManialink, LandmarkKind, Manialink, MarkerAnchor, MarkerBinding,
            MarkerLabel, MarkerVisibility, Placement, UiLayer,
        };
        let audiences = [
            Audience::Everyone,
            Audience::Players,
            Audience::Spectators,
            Audience::Teams(vec![1]),
            Audience::Accounts(vec!["a".into()]),
        ];
        for audience in audiences {
            let layer = UiLayer {
                manialink: Manialink::BuiltIn(BuiltInManialink::Standings),
                placement: Placement::Normal,
                audience,
            }
            .layer("");
            assert!(Driver::SCRIPT.contains(&format!("\"{}\"", layer.audience.kind)));
        }
        let labels = [
            MarkerLabel::Name,
            MarkerLabel::ClubTag,
            MarkerLabel::Position,
            MarkerLabel::Score,
            MarkerLabel::TeamShortName,
        ];
        let visibilities = [
            MarkerVisibility::WhenInFrustum,
            MarkerVisibility::WhenVisible,
            MarkerVisibility::WhenInMiddleOfScreen,
        ];
        for (label, visibility) in labels.into_iter().zip(visibilities.into_iter().cycle()) {
            let binding = MarkerBinding {
                frame_id: "Frame".into(),
                anchor: MarkerAnchor::Players(Audience::Everyone),
                label: Some(label),
                hide_on_self: false,
                max_distance: None,
                visibility,
            };
            let layer = UiLayer {
                manialink: Manialink::BuiltIn(BuiltInManialink::Standings),
                placement: Placement::Markers(vec![binding]),
                audience: Audience::Everyone,
            }
            .layer("");
            let marker = &layer.markers[0];
            assert!(Driver::SCRIPT.contains(&format!("\"{}\"", marker.label)));
            assert!(Driver::SCRIPT.contains(&format!("\"{}\"", marker.visibility)));
            assert!(Driver::SCRIPT.contains(&format!("\"{}\"", marker.anchor)));
        }
        // Landmarks are found by the tag of the map, not by a constant of the script.
        let _ = LandmarkKind::Checkpoint;
    }

    /// Every player learns which player and team they are, for the manialinks.
    #[test]
    fn script_writes_every_netvar_of_a_player() {
        for (name, kind) in crate::method::DRIVER_PLAYER_NETVARS {
            assert!(
                Driver::SCRIPT.contains(&format!("declare netwrite {kind} {name} for UI")),
                "{kind} {name} for UI is missing in TM_Driver.Script.txt"
            );
        }
    }

    /// The manialinks rely on the netvars and their types.
    #[test]
    fn script_writes_every_netvar() {
        for (name, kind) in crate::method::DRIVER_NETVARS {
            assert!(
                Driver::SCRIPT.contains(&format!("declare netwrite {kind} {name} for Teams[0]")),
                "{kind} {name} is missing in TM_Driver.Script.txt"
            );
        }
    }

    /// The script parses what the host publishes with `fromjson`, so its structs
    /// declare a member for every key of the json.
    #[cfg(feature = "serde")]
    #[test]
    fn script_parses_what_the_host_publishes() {
        use crate::method::{DriverNetRound, DriverNetStandings, DriverNetTeams, DriverNetUsers};

        fn keys(json: &serde_json::Value, found: &mut Vec<String>) {
            if let serde_json::Value::Object(object) = json {
                for (key, value) in object {
                    found.push(key.clone());
                    keys(value, found);
                }
            }
        }
        let mut found = Vec::new();
        keys(
            &serde_json::to_value(DriverNetStandings::default()).unwrap(),
            &mut found,
        );
        keys(
            &serde_json::to_value(DriverNetRound::default()).unwrap(),
            &mut found,
        );
        keys(
            &serde_json::to_value(DriverNetUsers::default()).unwrap(),
            &mut found,
        );
        keys(
            &serde_json::to_value(DriverNetTeams::default()).unwrap(),
            &mut found,
        );
        let marker = crate::method::DriverMarker {
            frame_id: String::new(),
            anchor: String::new(),
            players: Default::default(),
            position: Vec::new(),
            label: String::new(),
            hide_on_self: false,
            max_distance: 0,
            visibility: String::new(),
        };
        keys(&serde_json::to_value(marker).unwrap(), &mut found);
        for key in found {
            assert!(
                Driver::SCRIPT.contains(&format!(" {key};")),
                "No struct of TM_Driver.Script.txt has a member {key}"
            );
        }
    }

    /// Everything else reaches the driver through the standard callbacks.
    #[test]
    fn script_only_sends_holds() {
        assert_eq!(Driver::SCRIPT.matches("XmlRpc::SendCallback(").count(), 1);
    }

    #[test]
    fn decision_timeout_setting() {
        let driver = Driver {
            decision_timeout: 2500,
            ..Default::default()
        };
        assert!(
            driver
                .into_xml()
                .contains(r#"name="S_DriverDecisionTimeout" value="2500""#)
        );
    }
}
