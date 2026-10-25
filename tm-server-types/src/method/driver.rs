//! Commands understood by the generic driver modescript.
//!
//! Every command is sent as its own `TriggerModeScriptEventArray` call with text
//! parameters, the same convention Nadeo uses for the `Trackmania.*` methods.
//! [`DriverCommand::method_name`] and [`DriverCommand::params`] are the only place
//! that knows how a command looks on the wire.

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverCommand {
    /// Unblocks the modescript waiting on the decision callback with this `seq`.
    /// Sent by the driver host and never produced by a mode itself.
    Ack(u32),
    SetPlayerPoints(DriverPlayerPoints),
    /// Players that cannot spawn are out of the race (e.g. eliminated).
    SetCanSpawn(DriverCanSpawn),
    SetFinishTimeout(DriverFinishTimeout),
    ForceEndRound,
    /// Ends the map once the current decision is acknowledged.
    EndMap,
    /// Ends the match once the current decision is acknowledged.
    EndMatch,
    /// Shows a big message to everyone. An empty text hides it.
    SetMessage(String),
    /// Creates or replaces a manialink layer.
    SetLayer(DriverLayer),
    RemoveLayer(String),
    /// Text readable by manialinks through `netread Text Net_Driver_State for Teams[0]`.
    SetNetState(String),
    /// How the rounds starting from now on are played.
    SetRoundFormat(DriverRoundFormat),
}

impl DriverCommand {
    pub const ACK: &str = "Driver.Ack";
    pub const SET_PLAYER_POINTS: &str = "Driver.SetPlayerPoints";
    pub const SET_CAN_SPAWN: &str = "Driver.SetCanSpawn";
    pub const SET_FINISH_TIMEOUT: &str = "Driver.SetFinishTimeout";
    pub const FORCE_END_ROUND: &str = "Driver.ForceEndRound";
    pub const END_MAP: &str = "Driver.EndMap";
    pub const END_MATCH: &str = "Driver.EndMatch";
    pub const SET_MESSAGE: &str = "Driver.SetMessage";
    pub const SET_LAYER: &str = "Driver.SetLayer";
    pub const REMOVE_LAYER: &str = "Driver.RemoveLayer";
    pub const SET_NET_STATE: &str = "Driver.SetNetState";
    pub const SET_ROUND_FORMAT: &str = "Driver.SetRoundFormat";

    /// Name of the `TriggerModeScriptEventArray` method.
    pub fn method_name(&self) -> &'static str {
        match self {
            DriverCommand::Ack(_) => Self::ACK,
            DriverCommand::SetPlayerPoints(_) => Self::SET_PLAYER_POINTS,
            DriverCommand::SetCanSpawn(_) => Self::SET_CAN_SPAWN,
            DriverCommand::SetFinishTimeout(_) => Self::SET_FINISH_TIMEOUT,
            DriverCommand::ForceEndRound => Self::FORCE_END_ROUND,
            DriverCommand::EndMap => Self::END_MAP,
            DriverCommand::EndMatch => Self::END_MATCH,
            DriverCommand::SetMessage(_) => Self::SET_MESSAGE,
            DriverCommand::SetLayer(_) => Self::SET_LAYER,
            DriverCommand::RemoveLayer(_) => Self::REMOVE_LAYER,
            DriverCommand::SetNetState(_) => Self::SET_NET_STATE,
            DriverCommand::SetRoundFormat(_) => Self::SET_ROUND_FORMAT,
        }
    }

    /// Parameters in the order the modescript reads them.
    pub fn params(&self) -> Vec<String> {
        match self {
            DriverCommand::Ack(seq) => vec![seq.to_string()],
            DriverCommand::SetPlayerPoints(points) => vec![
                points.account_id.clone(),
                optional_points(points.round_points),
                optional_points(points.map_points),
                optional_points(points.match_points),
            ],
            DriverCommand::SetCanSpawn(can_spawn) => vec![
                can_spawn.account_id.clone(),
                if can_spawn.can_spawn { "1" } else { "0" }.to_string(),
            ],
            DriverCommand::SetFinishTimeout(timeout) => vec![match timeout {
                DriverFinishTimeout::Automatic => "auto".to_string(),
                DriverFinishTimeout::Disabled => "off".to_string(),
                DriverFinishTimeout::Milliseconds(ms) => ms.to_string(),
            }],
            DriverCommand::ForceEndRound | DriverCommand::EndMap | DriverCommand::EndMatch => {
                Vec::new()
            }
            DriverCommand::SetMessage(message) => vec![message.clone()],
            DriverCommand::SetLayer(layer) => vec![layer.name.clone(), layer.manialink.clone()],
            DriverCommand::RemoveLayer(name) => vec![name.clone()],
            DriverCommand::SetNetState(state) => vec![state.clone()],
            DriverCommand::SetRoundFormat(format) => match format {
                DriverRoundFormat::SingleAttempt => vec!["single_attempt".to_string()],
                DriverRoundFormat::TimeAttack(time_limit) => {
                    vec!["time_attack".to_string(), time_limit.to_string()]
                }
            },
        }
    }
}

/// An empty text keeps the current value in the modescript.
fn optional_points(points: Option<i32>) -> String {
    points.map(|p| p.to_string()).unwrap_or_default()
}

/// Points of a player as shown in the scores table. `None` keeps the current value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverPlayerPoints {
    pub account_id: String,
    pub round_points: Option<i32>,
    pub map_points: Option<i32>,
    pub match_points: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverCanSpawn {
    pub account_id: String,
    pub can_spawn: bool,
}

/// How long the remaining players have to finish once the first player finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverFinishTimeout {
    /// 5 seconds plus a sixth of the author time like the Nadeo modes.
    Automatic,
    /// The round ends once every player finished or gave up.
    Disabled,
    Milliseconds(u32),
}

/// How a round is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverRoundFormat {
    /// One attempt per player. The round ends once nobody is racing anymore or
    /// the finish timeout ran out. This is the default when a match starts.
    SingleAttempt,
    /// Players restart as often as they like and their best time counts.
    /// The round ends after this many milliseconds. 0 behaves like `SingleAttempt`.
    TimeAttack(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverLayer {
    pub name: String,
    pub manialink: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_points_keep_missing_values() {
        let command = DriverCommand::SetPlayerPoints(DriverPlayerPoints {
            account_id: "45b9cf1e-3c97-4753-ac63-ac61b48b4bb7".into(),
            round_points: Some(10),
            map_points: None,
            match_points: Some(-3),
        });

        assert_eq!(command.method_name(), "Driver.SetPlayerPoints");
        assert_eq!(
            command.params(),
            ["45b9cf1e-3c97-4753-ac63-ac61b48b4bb7", "10", "", "-3"]
        );
    }

    #[test]
    fn simple_params() {
        assert_eq!(DriverCommand::Ack(12).params(), ["12"]);
        assert!(DriverCommand::EndMatch.params().is_empty());
        assert_eq!(
            DriverCommand::SetCanSpawn(DriverCanSpawn {
                account_id: "a".into(),
                can_spawn: false
            })
            .params(),
            ["a", "0"]
        );
        assert_eq!(
            DriverCommand::SetFinishTimeout(DriverFinishTimeout::Automatic).params(),
            ["auto"]
        );
        assert_eq!(
            DriverCommand::SetFinishTimeout(DriverFinishTimeout::Disabled).params(),
            ["off"]
        );
        assert_eq!(
            DriverCommand::SetFinishTimeout(DriverFinishTimeout::Milliseconds(7000)).params(),
            ["7000"]
        );
        assert_eq!(
            DriverCommand::SetRoundFormat(DriverRoundFormat::SingleAttempt).params(),
            ["single_attempt"]
        );
        assert_eq!(
            DriverCommand::SetRoundFormat(DriverRoundFormat::TimeAttack(300_000)).params(),
            ["time_attack", "300000"]
        );
    }
}
