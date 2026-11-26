use dxr::TryFromValue;
use serde::{Deserialize, Deserializer};

use crate::base::login_to_account_id;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Map {
    pub uid: String,
    pub name: String,
    pub filename: String,
    #[cfg_attr(
        feature = "serde",
        serde(rename = "author", deserialize_with = "author_login_to_id")
    )]
    pub author_account_id: String,
    #[cfg_attr(feature = "serde", serde(rename = "authornickname"))]
    pub author_nickname: String,
    pub environment: String,
    pub mood: String,
    #[cfg_attr(feature = "serde", serde(rename = "bronzetime"))]
    pub bronze_time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "silvertime"))]
    pub silver_time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "goldtime"))]
    pub gold_time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "authortime"))]
    pub author_time: u32,

    pub copperprice: u32,

    #[cfg_attr(feature = "serde", serde(rename = "laprace"))]
    pub lap_race: bool,

    #[cfg_attr(feature = "serde", serde(rename = "nblaps"))]
    pub number_laps: u32,

    #[cfg_attr(feature = "serde", serde(rename = "maptype"))]
    pub map_type: String,

    #[cfg_attr(feature = "serde", serde(rename = "mapstyle"))]
    pub map_style: String,
}

fn author_login_to_id<'de, D>(d: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let login = String::deserialize(d)?;
    // This is hardcoded because on old maps Nadeo maps were using no account.
    if login == "Nadeo" {
        // Account id of the Nadeo. account which is used now.
        return Ok("aa02b90e-0652-4a1c-b705-4677e2983003".into());
    }
    Ok(login_to_account_id(&login))
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MapInfo {
    pub name: String,

    pub uid: String,

    pub file_name: String,
}

impl dxr::TryFromValue for MapInfo {
    fn try_from_value(value: &dxr::Value) -> ::std::result::Result<MapInfo, dxr::Error> {
        use ::std::collections::HashMap;
        use ::std::string::String;
        use dxr::Value;
        let map: HashMap<String, Value> = HashMap::try_from_value(value)?;
        Ok(MapInfo {
            name: <String as TryFromValue>::try_from_value(
                map.get("Name")
                    .ok_or_else(|| dxr::Error::missing_field("MapInfo", "Name"))?,
            )?,
            uid: <String as TryFromValue>::try_from_value(
                map.get("Uid")
                    .ok_or_else(|| dxr::Error::missing_field("MapInfo", "Uid"))?,
            )?,
            file_name: <String as TryFromValue>::try_from_value(
                map.get("FileName")
                    .ok_or_else(|| dxr::Error::missing_field("MapInfo", "FileName"))?,
            )?,
        })
    }
}
