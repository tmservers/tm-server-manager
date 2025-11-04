use dxr::{TryFromParams, TryFromValue};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerChat {
    //TODO
    /*  #[cfg_attr(feature = "serde", serde(rename = "Login", deserialize_with = ""))]
    account_id: UbisoftId, */
    #[cfg_attr(feature = "serde", serde(rename = "Login"))]
    pub login: String,
    #[cfg_attr(feature = "serde", serde(rename = "Text"))]
    pub text: String,
    #[cfg_attr(feature = "serde", serde(rename = "IsRegisteredCmd"))]
    pub is_registered_cmd: bool,
    #[cfg_attr(feature = "serde", serde(rename = "Options"))]
    pub options: i32,
}

impl TryFromParams for PlayerChat {
    fn try_from_params(values: &[dxr::Value]) -> Result<Self, dxr::DxrError> {
        Ok(Self {
            login: String::try_from_value(&values[1]).unwrap(),
            text: String::try_from_value(&values[2]).unwrap(),
            is_registered_cmd: bool::try_from_value(&values[3]).unwrap(),
            options: i32::try_from_value(&values[4]).unwrap(),
        })
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerConnect {
    //TODO
    /*  #[cfg_attr(feature = "serde", serde(rename = "Login", deserialize_with = ""))]
    account_id: UbisoftId, */
    #[cfg_attr(feature = "serde", serde(rename = "Login"))]
    pub login: String,
    #[cfg_attr(feature = "serde", serde(rename = "IsSpectator"))]
    pub is_spectator: bool,
}

impl TryFromParams for PlayerConnect {
    fn try_from_params(values: &[dxr::Value]) -> Result<Self, dxr::DxrError> {
        Ok(Self {
            login: String::try_from_value(&values[0]).unwrap(),
            is_spectator: bool::try_from_value(&values[1]).unwrap(),
        })
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerDisconnect {
    //TODO
    /*  #[cfg_attr(feature = "serde", serde(rename = "Login", deserialize_with = ""))]
    account_id: UbisoftId, */
    #[cfg_attr(feature = "serde", serde(rename = "Login"))]
    pub login: String,
    #[cfg_attr(feature = "serde", serde(rename = "DisconnectReason"))]
    pub disconnect_reason: String,
}

impl TryFromParams for PlayerDisconnect {
    fn try_from_params(values: &[dxr::Value]) -> Result<Self, dxr::DxrError> {
        Ok(Self {
            login: String::try_from_value(&values[0]).unwrap(),
            disconnect_reason: String::try_from_value(&values[1]).unwrap(),
        })
    }
}
