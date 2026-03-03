#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum RoundsPerMap {
    /// -1 or 0 = unlimited
    Unlimited,
    Rounds(u32),
}

impl From<i32> for RoundsPerMap {
    fn from(value: i32) -> Self {
        match value {
            0 | -1 => RoundsPerMap::Unlimited,
            _ => RoundsPerMap::Rounds(value as u32),
        }
    }
}

impl From<RoundsPerMap> for i32 {
    fn from(value: RoundsPerMap) -> Self {
        match value {
            RoundsPerMap::Unlimited => -1,
            RoundsPerMap::Rounds(s) => s as i32,
        }
    }
}
