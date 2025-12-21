#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Respawn {
    #[cfg_attr(feature = "serde", serde(rename = "accountid"))]
    pub account_id: String,
    pub time: u32,

    #[cfg_attr(feature = "serde", serde(rename = "nbrespawns"))]
    pub number_respawns: u32,

    pub racetime: i32,
    pub laptime: i32,

    #[cfg_attr(feature = "serde", serde(rename = "checkpointinrace"))]
    pub checkpoint_in_race: i32,

    #[cfg_attr(feature = "serde", serde(rename = "checkpointinlap"))]
    pub checkpoint_in_lap: i32,

    pub speed: f32,
}
