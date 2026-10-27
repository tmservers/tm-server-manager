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
    /// The warmup of the built-in warmup before the first round of the map that starts.
    SetWarmUp(DriverWarmUp),
    /// Players that cannot spawn are out of the race (e.g. eliminated).
    SetCanSpawn(DriverCanSpawn),
    /// Ends the map once the current hold continues.
    EndMap,
    /// Plays the map with this uid after the current one. It has to be in the map
    /// list of the server.
    SetNextMap(String),
    /// Ends the match once the current hold continues.
    EndMatch,
    /// Creates or replaces a manialink layer.
    SetLayer(DriverLayer),
    RemoveLayer(String),
    /// Shows or hides a UI module of the game, e.g. its scores table.
    SetUiModuleVisible(DriverUiModule),
    /// Text readable by manialinks through `netread Text Net_Driver_State for Teams[0]`.
    /// For data of a mode the standings can not hold.
    SetNetState(String),
    /// [`DriverStandings`] as json, readable by manialinks through
    /// `netread Text Net_Driver_Standings for Teams[0]`. Sent by the driver host
    /// whenever the standings change and never produced by a mode itself.
    SetStandings(String),
    /// [`DriverRound`] as json, readable by manialinks through
    /// `netread Text Net_Driver_Round for Teams[0]`. Sent by the driver host
    /// whenever a round counted and never produced by a mode itself.
    SetRound(String),
    /// [`DriverUsers`] as json, readable by manialinks through
    /// `netread Text Net_Driver_Users for Teams[0]`. Sent by the driver host
    /// whenever it learns about a player and never produced by a mode itself.
    SetUsers(String),
    /// A message for everyone in the chat.
    ChatMessage(String),
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
    /// The standard `ChatSendServerMessage` method.
    ChatSendServerMessage(String),
    /// The standard `SetNextMapIdent` method.
    SetNextMapIdent(String),
}

impl DriverCall {
    /// The standard method that changes the UI modules of the game.
    pub const UI_MODULES_SET_PROPERTIES: &str = "Common.UIModules.SetProperties";
}

impl DriverCommand {
    pub const CONTINUE: &str = "Driver.Continue";
    pub const SET_ROUND_FORMAT: &str = "Driver.SetRoundFormat";
    pub const SET_CAN_SPAWN: &str = "Driver.SetCanSpawn";
    pub const END_MATCH: &str = "Driver.EndMatch";
    pub const SET_LAYER: &str = "Driver.SetLayer";
    pub const REMOVE_LAYER: &str = "Driver.RemoveLayer";
    pub const SET_NET_STATE: &str = "Driver.SetNetState";
    pub const SET_STANDINGS: &str = "Driver.SetStandings";
    pub const SET_ROUND: &str = "Driver.SetRound";
    pub const SET_USERS: &str = "Driver.SetUsers";
    pub const SET_WARM_UP: &str = "Driver.SetWarmUp";

    /// Methods implemented by the driver modescript.
    pub const MODESCRIPT_METHODS: [&str; 11] = [
        Self::CONTINUE,
        Self::SET_ROUND_FORMAT,
        Self::SET_CAN_SPAWN,
        Self::END_MATCH,
        Self::SET_LAYER,
        Self::REMOVE_LAYER,
        Self::SET_NET_STATE,
        Self::SET_STANDINGS,
        Self::SET_ROUND,
        Self::SET_USERS,
        Self::SET_WARM_UP,
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
            DriverCommand::SetWarmUp(warm_up) => (
                Self::SET_WARM_UP,
                vec![
                    warm_up.rounds.to_string(),
                    warm_up.duration_ms.to_string(),
                    match warm_up.timeout_ms {
                        Some(ms) => ms.to_string(),
                        None => "auto".to_string(),
                    },
                ],
            ),
            DriverCommand::SetCanSpawn(can_spawn) => (
                Self::SET_CAN_SPAWN,
                vec![
                    can_spawn.account_id.clone(),
                    if can_spawn.can_spawn { "1" } else { "0" }.to_string(),
                ],
            ),
            DriverCommand::EndMap => return DriverCall::NextMap,
            DriverCommand::SetNextMap(uid) => return DriverCall::SetNextMapIdent(uid.clone()),
            DriverCommand::EndMatch => (Self::END_MATCH, Vec::new()),
            DriverCommand::SetLayer(layer) => (
                Self::SET_LAYER,
                vec![
                    layer.name.clone(),
                    layer.manialink.clone(),
                    match layer.kind {
                        DriverLayerKind::Normal => "normal",
                        DriverLayerKind::ScoresTable => "scores_table",
                    }
                    .to_string(),
                ],
            ),
            DriverCommand::RemoveLayer(name) => (Self::REMOVE_LAYER, vec![name.clone()]),
            DriverCommand::SetUiModuleVisible(module) => (
                DriverCall::UI_MODULES_SET_PROPERTIES,
                vec![format!(
                    r#"{{"uimodules":[{{"id":"{}","visible":{},"visible_update":true}}]}}"#,
                    module.id.replace('\\', "\\\\").replace('"', "\\\""),
                    module.visible
                )],
            ),
            DriverCommand::SetNetState(state) => (Self::SET_NET_STATE, vec![state.clone()]),
            DriverCommand::SetStandings(standings) => {
                (Self::SET_STANDINGS, vec![standings.clone()])
            }
            DriverCommand::SetRound(round) => (Self::SET_ROUND, vec![round.clone()]),
            DriverCommand::SetUsers(users) => (Self::SET_USERS, vec![users.clone()]),
            DriverCommand::ChatMessage(message) => {
                return DriverCall::ChatSendServerMessage(message.clone());
            }
        };
        DriverCall::ModeScript { method, params }
    }

    /// Shows these manialinks. The scores table of the game is hidden while one of
    /// them replaces it and shown again otherwise.
    pub fn show_layers(layers: &[DriverLayer]) -> Vec<DriverCommand> {
        let replaced = layers
            .iter()
            .any(|layer| layer.kind == DriverLayerKind::ScoresTable);
        let mut commands: Vec<DriverCommand> = layers
            .iter()
            .cloned()
            .map(DriverCommand::SetLayer)
            .collect();
        commands.push(DriverCommand::SetUiModuleVisible(DriverUiModule {
            id: DriverUiModule::SCORES_TABLE.into(),
            visible: !replaced,
        }));
        commands
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

/// A warmup with the built-in warmup of the game.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverWarmUp {
    /// Warmup rounds, 0 for none.
    pub rounds: u32,
    /// How long one warmup round lasts. 0 lets every player drive once.
    pub duration_ms: u32,
    /// Time to finish after the first player finished in rounds of one try.
    /// None computes it from the author time like the finish timeout.
    pub timeout_ms: Option<u32>,
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
    pub kind: DriverLayerKind,
}

impl DriverLayer {
    /// Manialinks whose name ends with this replace the scores table of the game.
    pub const SCORES_TABLE_SUFFIX: &str = "scores-table";

    /// A layer for a manialink selected by name, e.g. in the config. A name ending
    /// with `scores-table` makes it the scores table.
    pub fn selected(name: impl Into<String>, manialink: impl Into<String>) -> Self {
        let name = name.into();
        let kind = if name.ends_with(Self::SCORES_TABLE_SUFFIX) {
            DriverLayerKind::ScoresTable
        } else {
            DriverLayerKind::Normal
        };
        Self {
            name,
            manialink: manialink.into(),
            kind,
        }
    }
}

/// How the game shows a layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverLayerKind {
    /// Always shown.
    #[default]
    Normal,
    /// Shown while a player holds the scores table key and whenever the modescript
    /// forces the scores table, e.g. at the end of a round.
    ScoresTable,
}

/// A UI module of the game, see `Common.UIModules.SetProperties`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverUiModule {
    /// Id of the module, e.g. `Race_ScoresTable`.
    pub id: String,
    pub visible: bool,
}

impl DriverUiModule {
    /// The scores table of the game.
    pub const SCORES_TABLE: &str = "Race_ScoresTable";
}

/// The standings of a driver match as the manialinks get them through
/// `Net_Driver_Standings`. Every driver mode works on these: the driver host keeps
/// them, hands them to the mode with every event and shows what the mode returns
/// to the players. Manialinks parse them as json, so the field names must not change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverStandings {
    /// Name of the mode.
    pub title: String,
    /// Progress of the match as text, e.g. `2 / 5 rounds`.
    pub info: String,
    /// Rounds of the current map that counted.
    pub rounds_on_map: u32,
    /// Maps that ended in this match.
    pub maps_played: u32,
    /// Name of the current map.
    pub map_name: String,
    /// Maps in the playlist of the match, 0 without one.
    pub map_count: u32,
    /// Ordered by position, unranked players last.
    pub entries: Vec<DriverEntry>,
}

/// The last round of a driver match as the manialinks get it through
/// `Net_Driver_Round`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRound {
    /// Counted rounds of the match, from 1. 0 before the first round ended.
    pub number: u32,
    /// Ordered by place.
    pub entries: Vec<DriverEntry>,
}

/// A player in the standings of a driver match or of a round.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverEntry {
    pub account_id: String,
    /// Rank in the match or place in the round, from 1. Players with the same
    /// position are tied. 0 if the mode did not set it.
    pub position: u32,
    /// Points of the match or of the round.
    pub score: i32,
    /// E.g. the race that counted in the round in milliseconds, -1 without a time.
    pub time: i32,
    pub status: DriverStatus,
    /// Shown in place of the points, e.g. `Last Chance`. Empty without a label.
    pub label: String,
    pub label_tone: DriverLabelTone,
}

/// The players a driver host knows as the manialinks get them through
/// `Net_Driver_Users`, so the standings do not repeat the names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverUsers {
    pub users: Vec<DriverUser>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverUser {
    pub account_id: String,
    /// Empty until the host got the name from the server.
    pub name: String,
    /// Empty while unknown.
    pub club_tag: String,
    /// E.g. `World|Europe|Germany`. Empty while unknown.
    pub zone: String,
}

/// Whether a player takes part in a driver match and can still race. Who wins is
/// told by the position, what a mode calls a state by the label.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverStatus {
    /// Takes part and can still race.
    #[default]
    Playing,
    /// Out of the match with a final position.
    Eliminated,
    /// Takes no part in the match, e.g. joined too late. Not ranked.
    Spectator,
}

/// How the label of a standing is shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverLabelTone {
    #[default]
    Neutral,
    /// Something good, e.g. a win.
    Positive,
    /// Something bad, e.g. being one round from elimination.
    Negative,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modescript(command: DriverCommand) -> (&'static str, Vec<String>) {
        match command.call() {
            DriverCall::ModeScript { method, params } => (method, params),
            DriverCall::NextMap
            | DriverCall::ChatSendServerMessage(_)
            | DriverCall::SetNextMapIdent(_) => panic!("Expected a modescript method"),
        }
    }

    #[test]
    fn layers() {
        assert_eq!(
            modescript(DriverCommand::SetLayer(DriverLayer::selected(
                "standings",
                "<manialink/>"
            ))),
            (
                "Driver.SetLayer",
                vec!["standings".into(), "<manialink/>".into(), "normal".into()]
            )
        );
        assert_eq!(
            modescript(DriverCommand::SetLayer(DriverLayer::selected(
                "scores-table",
                "<manialink/>"
            )))
            .1[2],
            "scores_table"
        );
        assert_eq!(
            DriverLayer::selected("teams-scores-table", "").kind,
            DriverLayerKind::ScoresTable
        );
    }

    #[test]
    fn ui_modules_use_the_standard_method() {
        assert_eq!(
            modescript(DriverCommand::SetUiModuleVisible(DriverUiModule {
                id: DriverUiModule::SCORES_TABLE.into(),
                visible: false,
            })),
            (
                "Common.UIModules.SetProperties",
                vec![
                    r#"{"uimodules":[{"id":"Race_ScoresTable","visible":false,"visible_update":true}]}"#
                        .into()
                ]
            )
        );
    }

    #[test]
    fn a_scores_table_manialink_hides_the_one_of_the_game() {
        let scores_table = |visible| {
            DriverCommand::SetUiModuleVisible(DriverUiModule {
                id: "Race_ScoresTable".into(),
                visible,
            })
        };
        let standings = DriverLayer::selected("standings", "");
        let replacement = DriverLayer::selected("scores-table", "");

        assert_eq!(
            DriverCommand::show_layers(&[standings.clone(), replacement.clone()]),
            [
                DriverCommand::SetLayer(standings.clone()),
                DriverCommand::SetLayer(replacement),
                scores_table(false),
            ]
        );
        assert_eq!(
            DriverCommand::show_layers(std::slice::from_ref(&standings)),
            [DriverCommand::SetLayer(standings), scores_table(true)]
        );
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
    fn warm_ups() {
        assert_eq!(
            modescript(DriverCommand::SetWarmUp(DriverWarmUp {
                rounds: 2,
                duration_ms: 60_000,
                timeout_ms: None,
            })),
            (
                "Driver.SetWarmUp",
                vec!["2".into(), "60000".into(), "auto".into()]
            )
        );
        assert_eq!(
            modescript(DriverCommand::SetWarmUp(DriverWarmUp {
                rounds: 1,
                duration_ms: 0,
                timeout_ms: Some(5000),
            }))
            .1,
            ["1", "0", "5000"]
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
    fn standings_and_net_state() {
        assert_eq!(
            modescript(DriverCommand::SetStandings("{}".into())),
            ("Driver.SetStandings", vec!["{}".into()])
        );
        assert_eq!(
            modescript(DriverCommand::SetRound("{}".into())),
            ("Driver.SetRound", vec!["{}".into()])
        );
        assert_eq!(
            modescript(DriverCommand::SetUsers("{}".into())),
            ("Driver.SetUsers", vec!["{}".into()])
        );
        assert_eq!(
            modescript(DriverCommand::SetNetState("{}".into())),
            ("Driver.SetNetState", vec!["{}".into()])
        );
    }

    #[test]
    fn standard_methods() {
        assert_eq!(DriverCommand::EndMap.call(), DriverCall::NextMap);
        assert_eq!(
            DriverCommand::SetNextMap("uid".into()).call(),
            DriverCall::SetNextMapIdent("uid".into())
        );
        assert_eq!(
            DriverCommand::ChatMessage("hi".into()).call(),
            DriverCall::ChatSendServerMessage("hi".into())
        );
    }
}
