use dxr::TryFromValue;

use crate::{
    base::{RoundTime, login_to_account_id},
    event::Event,
};

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
    round_points: i32,
    #[cfg_attr(feature = "serde", serde(rename = "mappoints"))]
    map_points: i32,
    #[cfg_attr(feature = "serde", serde(rename = "matchpoints"))]
    match_points: i32,

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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerInfo {
    pub account_id: String,

    pub nick_name: String,

    pub player_id: i32,
    pub team_id: i32,
    pub spectator_status: i32,
    pub ladder_ranking: i32,
    pub flags: i32,
}

impl dxr::TryFromValue for PlayerInfo {
    fn try_from_value(value: &dxr::Value) -> ::std::result::Result<PlayerInfo, dxr::Error> {
        use ::std::collections::HashMap;
        use ::std::string::String;
        use dxr::Value;
        let map: HashMap<String, Value> = HashMap::try_from_value(value)?;
        Ok(PlayerInfo {
            account_id: login_to_account_id(&<String as TryFromValue>::try_from_value(
                map.get("Login")
                    .ok_or_else(|| dxr::Error::missing_field("PlayerInfo", "Login"))?,
            )?),
            nick_name: <String as TryFromValue>::try_from_value(
                map.get("NickName")
                    .ok_or_else(|| dxr::Error::missing_field("PlayerInfo", "NickName"))?,
            )?,
            player_id: <i32 as TryFromValue>::try_from_value(
                map.get("PlayerId")
                    .ok_or_else(|| dxr::Error::missing_field("PlayerInfo", "PlayerId"))?,
            )?,
            team_id: <i32 as TryFromValue>::try_from_value(
                map.get("TeamId")
                    .ok_or_else(|| dxr::Error::missing_field("PlayerInfo", "TeamId"))?,
            )?,
            spectator_status: <i32 as TryFromValue>::try_from_value(
                map.get("SpectatorStatus")
                    .ok_or_else(|| dxr::Error::missing_field("PlayerInfo", "SpectatorStatus"))?,
            )?,
            ladder_ranking: <i32 as TryFromValue>::try_from_value(
                map.get("LadderRanking")
                    .ok_or_else(|| dxr::Error::missing_field("PlayerInfo", "LadderRanking"))?,
            )?,
            flags: <i32 as TryFromValue>::try_from_value(
                map.get("Flags")
                    .ok_or_else(|| dxr::Error::missing_field("PlayerInfo", "Flags"))?,
            )?,
        })
    }
}

impl<'a> From<&'a Event> for &'a PlayerInfo {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::PlayerInfoChanged(event) => event,
            _ => unreachable!(),
        }
    }
}
