//! Settings shared by modes.

use serde::Deserialize;

use crate::{FinishTimeout, WarmUp};

/// A finish timeout in the json settings of a mode:
/// `"automatic"`, `"disabled"` or `{"milliseconds": 5000}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishTimeoutSetting {
    Automatic,
    Disabled,
    Milliseconds(u32),
}

/// The warmup before the first round of every map in the json settings of a mode,
/// e.g. `{"rounds": 1, "duration_ms": 60000}`. No warmup by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WarmUpSetting {
    /// Warmup rounds, 0 for none.
    pub rounds: u32,
    /// How long one warmup round lasts. 0 lets every player drive once.
    pub duration_ms: u32,
    /// Time to finish after the first player finished in rounds of one try.
    /// Computed from the author time without it.
    pub timeout_ms: Option<u32>,
}

impl From<WarmUpSetting> for WarmUp {
    fn from(value: WarmUpSetting) -> Self {
        WarmUp {
            rounds: value.rounds,
            duration_ms: value.duration_ms,
            timeout_ms: value.timeout_ms,
        }
    }
}

impl From<FinishTimeoutSetting> for FinishTimeout {
    fn from(value: FinishTimeoutSetting) -> Self {
        match value {
            FinishTimeoutSetting::Automatic => FinishTimeout::Automatic,
            FinishTimeoutSetting::Disabled => FinishTimeout::Disabled,
            FinishTimeoutSetting::Milliseconds(ms) => FinishTimeout::Milliseconds(ms),
        }
    }
}
