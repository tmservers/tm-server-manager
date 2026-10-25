use std::collections::BTreeMap;

/// The generic driver modescript. The mode logic runs in a wasm blob inside the bridge.
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::DriverEvent, method::DriverCommand};

    /// The modescript and the Rust types have to agree on every name.
    #[test]
    fn script_knows_every_callback_and_command() {
        let names = [
            DriverEvent::HELLO,
            DriverEvent::MATCH_START,
            DriverEvent::MAP_START,
            DriverEvent::ROUND_START,
            DriverEvent::ROUND_END,
            DriverEvent::MAP_END,
            DriverEvent::MATCH_END,
            DriverEvent::PLAYER_JOIN,
            DriverEvent::PLAYER_LEAVE,
            DriverCommand::ACK,
            DriverCommand::SET_PLAYER_POINTS,
            DriverCommand::SET_CAN_SPAWN,
            DriverCommand::SET_FINISH_TIMEOUT,
            DriverCommand::FORCE_END_ROUND,
            DriverCommand::END_MAP,
            DriverCommand::END_MATCH,
            DriverCommand::SET_MESSAGE,
            DriverCommand::SET_LAYER,
            DriverCommand::REMOVE_LAYER,
            DriverCommand::SET_NET_STATE,
        ];
        for name in names {
            assert!(
                Driver::SCRIPT.contains(&format!("\"{name}\"")),
                "{name} is missing in TM_Driver.Script.txt"
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
