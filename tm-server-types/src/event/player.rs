use dxr::{TryFromParams, TryFromValue};

use crate::{base::login_to_account_id, event::Event};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerChat {
    #[cfg_attr(feature = "serde", serde(rename = "Login"))]
    pub account_id: String,
    #[cfg_attr(feature = "serde", serde(rename = "Text"))]
    pub text: String,
    #[cfg_attr(feature = "serde", serde(rename = "IsRegisteredCmd"))]
    pub is_registered_cmd: bool,
    #[cfg_attr(feature = "serde", serde(rename = "Options"))]
    pub options: i32,
}

impl TryFromParams for PlayerChat {
    fn try_from_params(values: &[dxr::Value]) -> Result<Self, dxr::Error> {
        Ok(Self {
            account_id: login_to_account_id(&String::try_from_value(&values[1])?),
            text: String::try_from_value(&values[2])?,
            is_registered_cmd: bool::try_from_value(&values[3])?,
            options: i32::try_from_value(&values[4])?,
        })
    }
}

impl<'a> From<&'a Event> for &'a PlayerChat {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::PlayerChat(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerConnect {
    #[cfg_attr(feature = "serde", serde(rename = "Login"))]
    pub account_id: String,
    #[cfg_attr(feature = "serde", serde(rename = "IsSpectator"))]
    pub is_spectator: bool,
}

impl TryFromParams for PlayerConnect {
    fn try_from_params(values: &[dxr::Value]) -> Result<Self, dxr::Error> {
        Ok(Self {
            account_id: login_to_account_id(&String::try_from_value(&values[0])?),
            is_spectator: bool::try_from_value(&values[1])?,
        })
    }
}

impl<'a> From<&'a Event> for &'a PlayerConnect {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::PlayerConnect(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayerDisconnect {
    #[cfg_attr(feature = "serde", serde(rename = "Login"))]
    pub account_id: String,
    #[cfg_attr(feature = "serde", serde(rename = "DisconnectReason"))]
    pub disconnect_reason: String,
}

impl TryFromParams for PlayerDisconnect {
    fn try_from_params(values: &[dxr::Value]) -> Result<Self, dxr::Error> {
        Ok(Self {
            account_id: login_to_account_id(&String::try_from_value(&values[0])?),
            disconnect_reason: String::try_from_value(&values[1])?,
        })
    }
}

impl<'a> From<&'a Event> for &'a PlayerDisconnect {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::PlayerDisconnect(event) => event,
            _ => unreachable!(),
        }
    }
}
