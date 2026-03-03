#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum WarmupDuration {
    /// Only one try like a round
    OneTry,
    // Time based on the Author medal ( 5 seconds + Author Time on 1 lap + ( Author Time on 1 lap / 6 ) )
    BasedOnMedal,
    /// Time in seconds
    Seconds(u32),
}

impl From<i32> for WarmupDuration {
    fn from(value: i32) -> Self {
        match value {
            -1 => WarmupDuration::OneTry,
            0 => WarmupDuration::BasedOnMedal,
            _ => WarmupDuration::Seconds(value as u32),
        }
    }
}

impl From<WarmupDuration> for i32 {
    fn from(value: WarmupDuration) -> Self {
        match value {
            WarmupDuration::OneTry => -1,
            WarmupDuration::BasedOnMedal => 0,
            WarmupDuration::Seconds(s) => s as i32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
#[cfg_attr(feature = "serde", serde(from = "i32", into = "i32"))]
pub enum WarmupTimeout {
    /// Time based on the Author medal ( 5 seconds + Author time / 6 )
    BasedOnMedal,
    /// Time in seconds
    Seconds(u32),
}

impl From<i32> for WarmupTimeout {
    fn from(value: i32) -> Self {
        match value {
            -1 => WarmupTimeout::BasedOnMedal,
            _ => WarmupTimeout::Seconds(value as u32),
        }
    }
}

impl From<WarmupTimeout> for i32 {
    fn from(value: WarmupTimeout) -> Self {
        match value {
            WarmupTimeout::BasedOnMedal => -1,
            WarmupTimeout::Seconds(s) => s as i32,
        }
    }
}
