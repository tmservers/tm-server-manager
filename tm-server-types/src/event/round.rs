use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct StartRound {
    pub count: u32,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a StartRound {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::StartRoundStart(event) => event,
            Event::StartRoundEnd(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndRoundStart {
    pub count: u32,
    pub valid: u32,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a EndRoundStart {
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndRoundStart(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndRoundEnd {
    pub count: u32,
    pub valid: u32,
    pub time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "isvalid"))]
    pub is_valid: bool,
}

impl<'a> From<&'a Event> for &'a EndRoundEnd {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndRoundEnd(event) => event,
            _ => unreachable!(),
        }
    }
}
