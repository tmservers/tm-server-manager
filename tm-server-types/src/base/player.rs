use dxr::TryFromValue;

use crate::base::RoundTime;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Player {
    #[cfg_attr(feature = "serde", serde(rename = "accountid"))]
    account_id: String,

    name: String,

    team: i32,

    rank: u32,

    #[cfg_attr(feature = "serde", serde(rename = "roundpoints"))]
    round_points: u32,
    #[cfg_attr(feature = "serde", serde(rename = "mappoints"))]
    map_points: u32,
    #[cfg_attr(feature = "serde", serde(rename = "matchpoints"))]
    match_points: u32,

    #[cfg_attr(feature = "serde", serde(rename = "bestracetime"))]
    best_racetime: RoundTime,

    #[cfg_attr(feature = "serde", serde(rename = "bestracecheckpoints"))]
    best_race_checkpoints: Vec<u32>,
    #[cfg_attr(feature = "serde", serde(rename = "bestlaptime"))]
    best_laptime: RoundTime,
    #[cfg_attr(feature = "serde", serde(rename = "bestlapcheckpoints"))]
    best_lap_checkpoints: Vec<u32>,
    #[cfg_attr(feature = "serde", serde(rename = "prevracetime"))]
    previous_racetime: RoundTime,
    #[cfg_attr(feature = "serde", serde(rename = "prevracecheckpoints"))]
    previous_race_checkpoints: Vec<u32>,
}

#[derive(Debug, Clone, TryFromValue)]
//#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
//#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
//#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerInfo {
    pub Login: String,

    pub NickName: String,

    pub PlayerId: i32,
    pub TeamId: i32,
    pub SpectatorStatus: i32,
    pub LadderRanking: i32,
    pub Flags: i32,
}
