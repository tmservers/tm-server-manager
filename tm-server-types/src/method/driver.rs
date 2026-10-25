//! Commands of the wasm driver for the server.
//!
//! The driver modescript only implements what is not available through the
//! standard XML-RPC methods already. [`DriverCommand::call`] is the only place
//! that knows how a command is sent.

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverCommand {
    /// Lets the modescript continue after the hold with this `seq`.
    /// Sent by the driver host and never produced by a mode itself.
    Continue(u32),
    /// How the rounds starting from now on are played.
    SetRoundFormat(DriverRoundFormat),
    /// Players that cannot spawn are out of the race (e.g. eliminated).
    SetCanSpawn(DriverCanSpawn),
    /// Ends the map once the current hold continues.
    EndMap,
    /// Ends the match once the current hold continues.
    EndMatch,
    /// Creates or replaces a manialink layer.
    SetLayer(DriverLayer),
    RemoveLayer(String),
    /// Text readable by manialinks through `netread Text Net_Driver_State for Teams[0]`.
    SetNetState(String),
}

/// How a command reaches the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverCall {
    /// A `TriggerModeScriptEventArray` method of the driver modescript.
    ModeScript {
        method: &'static str,
        params: Vec<String>,
    },
    /// The standard `NextMap` method.
    NextMap,
}

impl DriverCommand {
    pub const CONTINUE: &str = "Driver.Continue";
    pub const SET_ROUND_FORMAT: &str = "Driver.SetRoundFormat";
    pub const SET_CAN_SPAWN: &str = "Driver.SetCanSpawn";
    pub const END_MATCH: &str = "Driver.EndMatch";
    pub const SET_LAYER: &str = "Driver.SetLayer";
    pub const REMOVE_LAYER: &str = "Driver.RemoveLayer";
    pub const SET_NET_STATE: &str = "Driver.SetNetState";

    /// Methods implemented by the driver modescript.
    pub const MODESCRIPT_METHODS: [&str; 7] = [
        Self::CONTINUE,
        Self::SET_ROUND_FORMAT,
        Self::SET_CAN_SPAWN,
        Self::END_MATCH,
        Self::SET_LAYER,
        Self::REMOVE_LAYER,
        Self::SET_NET_STATE,
    ];

    pub fn call(&self) -> DriverCall {
        let (method, params) = match self {
            DriverCommand::Continue(seq) => (Self::CONTINUE, vec![seq.to_string()]),
            DriverCommand::SetRoundFormat(format) => (
                Self::SET_ROUND_FORMAT,
                match format {
                    DriverRoundFormat::SingleAttempt(timeout) => vec![
                        "single_attempt".to_string(),
                        match timeout {
                            DriverFinishTimeout::Automatic => "auto".to_string(),
                            DriverFinishTimeout::Disabled => "off".to_string(),
                            DriverFinishTimeout::Milliseconds(ms) => ms.to_string(),
                        },
                    ],
                    DriverRoundFormat::TimeAttack(time_limit) => {
                        vec!["time_attack".to_string(), time_limit.to_string()]
                    }
                },
            ),
            DriverCommand::SetCanSpawn(can_spawn) => (
                Self::SET_CAN_SPAWN,
                vec![
                    can_spawn.account_id.clone(),
                    if can_spawn.can_spawn { "1" } else { "0" }.to_string(),
                ],
            ),
            DriverCommand::EndMap => return DriverCall::NextMap,
            DriverCommand::EndMatch => (Self::END_MATCH, Vec::new()),
            DriverCommand::SetLayer(layer) => (
                Self::SET_LAYER,
                vec![layer.name.clone(), layer.manialink.clone()],
            ),
            DriverCommand::RemoveLayer(name) => (Self::REMOVE_LAYER, vec![name.clone()]),
            DriverCommand::SetNetState(state) => (Self::SET_NET_STATE, vec![state.clone()]),
        };
        DriverCall::ModeScript { method, params }
    }
}

/// How a round is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverRoundFormat {
    /// One attempt per player. The round ends once nobody is racing anymore or
    /// the finish timeout ran out. Automatic is the default when a match starts.
    SingleAttempt(DriverFinishTimeout),
    /// Players restart as often as they like and their best time counts.
    /// The round ends after this many milliseconds. 0 behaves like a single attempt.
    TimeAttack(u32),
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

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverCanSpawn {
    pub account_id: String,
    pub can_spawn: bool,
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

    fn modescript(command: DriverCommand) -> (&'static str, Vec<String>) {
        match command.call() {
            DriverCall::ModeScript { method, params } => (method, params),
            DriverCall::NextMap => panic!("Expected a modescript method"),
        }
    }

    #[test]
    fn round_formats() {
        assert_eq!(
            modescript(DriverCommand::SetRoundFormat(
                DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Automatic)
            )),
            (
                "Driver.SetRoundFormat",
                vec!["single_attempt".into(), "auto".into()]
            )
        );
        assert_eq!(
            modescript(DriverCommand::SetRoundFormat(
                DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Milliseconds(7000))
            ))
            .1,
            ["single_attempt", "7000"]
        );
        assert_eq!(
            modescript(DriverCommand::SetRoundFormat(
                DriverRoundFormat::TimeAttack(300_000)
            ))
            .1,
            ["time_attack", "300000"]
        );
    }

    #[test]
    fn simple_params() {
        assert_eq!(
            modescript(DriverCommand::Continue(12)),
            ("Driver.Continue", vec!["12".into()])
        );
        assert!(modescript(DriverCommand::EndMatch).1.is_empty());
        assert_eq!(
            modescript(DriverCommand::SetCanSpawn(DriverCanSpawn {
                account_id: "a".into(),
                can_spawn: false
            }))
            .1,
            ["a", "0"]
        );
    }

    #[test]
    fn end_map_uses_next_map() {
        assert_eq!(DriverCommand::EndMap.call(), DriverCall::NextMap);
    }
}
