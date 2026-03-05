use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Podium {
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a Podium {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::PodiumStart(event) => event,
            Event::PodiumEnd(event) => event,
            _ => unreachable!(),
        }
    }
}
