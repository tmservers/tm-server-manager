use crate::{base::Map, event::Event};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct LoadingMapStart {
    pub restarted: bool,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a LoadingMapStart {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::LoadingMapStart(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct LoadingMapEnd {
    pub restarted: bool,
    pub time: u32,
    pub map: Map,
}

impl<'a> From<&'a Event> for &'a LoadingMapEnd {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::LoadingMapEnd(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct StartMap {
    pub count: u32,
    pub valid: u32,
    pub restarted: bool,
    pub time: u32,
    pub map: Map,
}

impl<'a> From<&'a Event> for &'a StartMap {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::StartMapStart(event) => event,
            Event::StartMapEnd(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndMapStart {
    pub count: u32,
    pub valid: u32,

    pub map: Map,
}

impl<'a> From<&'a Event> for &'a EndMapStart {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndMapStart(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndMapEnd {
    pub count: u32,
    pub valid: u32,
    #[cfg_attr(feature = "serde", serde(rename = "isvalid"))]
    pub is_valid: bool,
    pub time: u32,

    pub map: Map,
}

impl<'a> From<&'a Event> for &'a EndMapEnd {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndMapEnd(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct UnloadingMapStart {
    pub time: u32,
    pub map: Map,
}

impl<'a> From<&'a Event> for &'a UnloadingMapStart {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::UnloadingMapStart(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct UnloadingMapEnd {
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a UnloadingMapEnd {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::UnloadingMapEnd(event) => event,
            _ => unreachable!(),
        }
    }
}
