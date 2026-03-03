#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum FinishTimeout {
    // Time based on the Author medal ( 5 seconds + Author time / 6 )
    BasedOnMedal,
    /// Time in seconds
    Seconds(u32),
}

impl From<i32> for FinishTimeout {
    fn from(value: i32) -> Self {
        match value {
            -1 => FinishTimeout::BasedOnMedal,
            _ => FinishTimeout::Seconds(value as u32),
        }
    }
}

impl From<FinishTimeout> for i32 {
    fn from(value: FinishTimeout) -> Self {
        match value {
            FinishTimeout::BasedOnMedal => -1,
            FinishTimeout::Seconds(s) => s as i32,
        }
    }
}
