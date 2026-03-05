use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct GiveUp {
    #[cfg_attr(feature = "serde", serde(rename = "accountid"))]
    pub account_id: String,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a GiveUp {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::GiveUp(event) => event,
            _ => unreachable!(),
        }
    }
}
