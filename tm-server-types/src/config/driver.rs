use std::collections::BTreeMap;

/// The generic driver modescript. The mode logic runs in a wasm blob, which the bridge is
/// going to run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Driver {
    /// Name of the driver mode e.g. `rounds`.
    pub mode: String,
    /// Json settings passed to the mode when it is initialized.
    pub settings: String,
    /// Milliseconds the modescript waits for a decision before showing a waiting
    /// message and asking again.
    pub decision_timeout: u32,
    /// Manialinks shown next to the mode, e.g. `standings`. The modescript keeps
    /// no points, so these show what the mode publishes through `Net_Driver_State`.
    pub manialinks: Vec<String>,
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
            mode: "rounds".into(),
            settings: "{}".into(),
            decision_timeout: 10_000,
            manialinks: vec!["standings".into(), "scores-table".into()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{DriverGhost, DriverHold},
        method::DriverCommand,
    };

    /// The modescript and the Rust types have to agree on every name.
    #[test]
    fn script_knows_every_callback_and_method() {
        for name in [DriverHold::CALLBACK, DriverGhost::CALLBACK]
            .into_iter()
            .chain(DriverCommand::MODESCRIPT_METHODS)
        {
            assert!(
                Driver::SCRIPT.contains(&format!("\"{name}\"")),
                "{name} is missing in TM_Driver.Script.txt"
            );
        }
    }

    /// Layers of the kind scores_table become the scores table of the game.
    #[test]
    fn script_knows_layer_kinds() {
        assert!(Driver::SCRIPT.contains("\"scores_table\""));
        assert!(Driver::SCRIPT.contains("CUILayer::EUILayerType::ScoresTable"));
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
        use crate::method::{DriverNetRound, DriverNetStandings, DriverNetUsers};

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
        for key in found {
            assert!(
                Driver::SCRIPT.contains(&format!(" {key};")),
                "No struct of TM_Driver.Script.txt has a member {key}"
            );
        }
    }

    /// Everything else reaches the driver through the standard callbacks.
    #[test]
    fn script_only_sends_holds_and_ghosts() {
        assert_eq!(Driver::SCRIPT.matches("XmlRpc::SendCallback(").count(), 2);
        assert!(Driver::SCRIPT.contains("XmlRpc::SendCallback(C_Callback_Hold,"));
        assert!(Driver::SCRIPT.contains("XmlRpc::SendCallback(C_Callback_Ghost,"));
    }

    /// The script sends ghosts with `tojson`, so its struct has a member for every
    /// field of [`DriverGhost`].
    #[cfg(feature = "serde")]
    #[test]
    fn script_sends_what_the_ghost_parses() {
        let ghost = DriverGhost {
            session: 0,
            round: 0,
            map_uid: String::new(),
            account_id: String::new(),
            time: 0,
            path: String::new(),
        };
        let serde_json::Value::Object(fields) = serde_json::to_value(ghost).unwrap() else {
            unreachable!("A struct serializes to an object");
        };
        let script_struct = Driver::SCRIPT
            .split("#Struct K_Driver_Ghost {")
            .nth(1)
            .and_then(|rest| rest.split('}').next())
            .expect("TM_Driver.Script.txt declares K_Driver_Ghost");
        for key in fields.keys() {
            assert!(
                script_struct.contains(&format!(" {key};")),
                "K_Driver_Ghost of TM_Driver.Script.txt has no member {key}"
            );
        }
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
