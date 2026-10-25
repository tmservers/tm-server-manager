//! Settings shared by modes.

use serde::Deserialize;

use crate::FinishTimeout;

/// A finish timeout in the json settings of a mode:
/// `"automatic"`, `"disabled"` or `{"milliseconds": 5000}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishTimeoutSetting {
    Automatic,
    Disabled,
    Milliseconds(u32),
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
