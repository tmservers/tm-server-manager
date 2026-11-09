use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct WayPoint {
    #[cfg_attr(feature = "serde", serde(rename = "accountid"))]
    pub account_id: String,
    pub time: u32,
    pub racetime: u32,
    pub laptime: u32,
    pub speed: f32,

    #[cfg_attr(
        feature = "serde",
        serde(rename = "checkpointinrace", deserialize_with = "negative_to_zero")
    )]
    pub checkpoint_in_race: u32,
    #[cfg_attr(
        feature = "serde",
        serde(rename = "checkpointinlap", deserialize_with = "negative_to_zero")
    )]
    pub checkpoint_in_lap: u32,
    #[cfg_attr(feature = "serde", serde(rename = "isendrace"))]
    pub is_end_race: bool,
    #[cfg_attr(feature = "serde", serde(rename = "isendlap"))]
    pub is_end_lap: bool,
    #[cfg_attr(feature = "serde", serde(rename = "isinfinitelaps"))]
    pub is_infinite_laps: bool,
    #[cfg_attr(feature = "serde", serde(rename = "isindependentlaps"))]
    pub is_independent_laps: bool,
    #[cfg_attr(feature = "serde", serde(rename = "curracecheckpoints"))]
    pub current_race_checkpoints: Vec<u32>,
    #[cfg_attr(feature = "serde", serde(rename = "curlapcheckpoints"))]
    pub current_lap_checkpoints: Vec<u32>,
    #[cfg_attr(feature = "serde", serde(rename = "blockid"))]
    pub block_id: String,
}

/// The server sometimes sends negative checkpoint indices, which count as 0.
#[cfg(feature = "serde")]
fn negative_to_zero<'de, D>(d: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::{Deserialize, de::Error};

    let index = i32::deserialize(d)?;
    u32::try_from(index.max(0)).map_err(D::Error::custom)
}

impl<'a> From<&'a Event> for &'a WayPoint {
    fn from(value: &'a Event) -> Self {
        match value {
            Event::WayPoint(event) => event,
            _ => unreachable!(),
        }
    }
}
