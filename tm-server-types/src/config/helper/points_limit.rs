#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum PointsLimit {
    /// 0 = unlimited for Champion & Rounds
    Unlimited,
    /// Points limit
    PointsLimit(u32),
}

impl From<i32> for PointsLimit {
    fn from(value: i32) -> Self {
        match value {
            0 => PointsLimit::Unlimited,
            _ => PointsLimit::PointsLimit(value as u32),
        }
    }
}

impl From<PointsLimit> for i32 {
    fn from(value: PointsLimit) -> Self {
        match value {
            PointsLimit::Unlimited => 0,
            PointsLimit::PointsLimit(s) => s as i32,
        }
    }
}
