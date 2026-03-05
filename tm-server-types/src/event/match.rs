use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct StartMatch {
    pub count: u32,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a StartMatch {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::StartMatchStart(event) => event,
            Event::StartMatchEnd(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndMatch {
    pub count: u32,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a EndMatch {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndMatchStart(event) => event,
            Event::EndMatchEnd(event) => event,
            _ => unreachable!(),
        }
    }
}
