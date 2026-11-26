//! Settings shared by modes.

use serde::{Deserialize, Serialize};

use crate::{FinishTimeout, Host};

/// A finish timeout in the json settings of a mode:
/// `"automatic"`, `"disabled"` or `{"milliseconds": 5000}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishTimeoutSetting {
    Automatic,
    Disabled,
    Milliseconds(u32),
}

/// The maps of a match by a `maps_per_match` setting: the playlist once without
/// the setting, one map without a playlist. 0 means no limit.
pub fn maps_per_match(host: &dyn Host, setting: Option<u32>) -> u32 {
    setting.unwrap_or_else(|| host.map_count().max(1))
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
