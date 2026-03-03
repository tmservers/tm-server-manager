#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum MapsPerMatch {
    /// Set 0 or -1 is equivalent to have only one map.
    One,
    Maps(u32),
}

impl From<i32> for MapsPerMatch {
    fn from(value: i32) -> Self {
        match value {
            0 | -1 => MapsPerMatch::One,
            _ => MapsPerMatch::Maps(value as u32),
        }
    }
}

impl From<MapsPerMatch> for i32 {
    fn from(value: MapsPerMatch) -> Self {
        match value {
            MapsPerMatch::One => -1,
            MapsPerMatch::Maps(s) => s as i32,
        }
    }
}
