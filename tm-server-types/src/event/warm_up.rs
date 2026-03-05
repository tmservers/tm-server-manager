use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct WarmupRound {
    pub current: u32,
    pub total: u32,
}

impl<'a> From<&'a Event> for &'a WarmupRound {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::WarmupStartRound(event) => event,
            Event::WarmupEndRound(event) => event,
            _ => unreachable!(),
        }
    }
}
