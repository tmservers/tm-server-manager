use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct StartServer {
    pub restarted: bool,
    pub time: u32,
    pub mode: ServerModeInfo,
}

impl<'a> From<&'a Event> for &'a StartServer {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::StartServerStart(event) => event,
            Event::StartServerEnd(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct ServerModeInfo {
    pub updated: bool,
    pub name: String,
}
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndServer {
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a EndServer {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndServerStart(event) => event,
            Event::EndServerEnd(event) => event,
            _ => unreachable!(),
        }
    }
}
