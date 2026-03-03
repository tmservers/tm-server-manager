#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum LapsNumber {
    /// Use laps from map validation
    Validation,
    // Independent laps (only useful in TimeAttack)
    Independent,
    /// Number of laps
    Laps(u32),
}

impl From<i32> for LapsNumber {
    fn from(value: i32) -> Self {
        match value {
            -1 => LapsNumber::Validation,
            0 => LapsNumber::Independent,
            _ => LapsNumber::Laps(value as u32),
        }
    }
}

impl From<LapsNumber> for i32 {
    fn from(value: LapsNumber) -> Self {
        match value {
            LapsNumber::Validation => -1,
            LapsNumber::Independent => 0,
            LapsNumber::Laps(s) => s as i32,
        }
    }
}
