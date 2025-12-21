#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Map {
    uid: String,
    name: String,
    filename: String,
    #[cfg_attr(feature = "serde", serde(rename = "author"))]
    author_login: String,
    #[cfg_attr(feature = "serde", serde(rename = "authornickname"))]
    author_nickname: String,
    environment: String,
    mood: String,
    #[cfg_attr(feature = "serde", serde(rename = "bronzetime"))]
    bronze_time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "silvertime"))]
    silver_time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "goldtime"))]
    gold_time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "authortime"))]
    author_time: u32,

    copperprice: u32,

    #[cfg_attr(feature = "serde", serde(rename = "laprace"))]
    lap_race: bool,

    #[cfg_attr(feature = "serde", serde(rename = "nblaps"))]
    number_laps: u32,

    #[cfg_attr(feature = "serde", serde(rename = "maptype"))]
    map_type: String,

    #[cfg_attr(feature = "serde", serde(rename = "mapstyle"))]
    map_style: String,
}
