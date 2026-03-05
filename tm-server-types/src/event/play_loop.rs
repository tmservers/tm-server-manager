use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayLoopStart {
    pub count: u32,
    pub valid: u32,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a PlayLoopStart {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::PlayLoopStart(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct PlayLoopEnd {
    pub count: u32,
    pub valid: u32,
    pub time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "isvalid"))]
    pub is_valid: bool,
}

impl<'a> From<&'a Event> for &'a PlayLoopEnd {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::PlayLoopEnd(event) => event,
            _ => unreachable!(),
        }
    }
}
