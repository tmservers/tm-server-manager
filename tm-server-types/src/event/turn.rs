use crate::event::Event;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct StartTurn {
    pub count: u32,
    pub valid: u32,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a StartTurn {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::StartTurnStart(event) => event,
            Event::StartTurnEnd(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndTurnStart {
    pub count: u32,
    pub valid: u32,
    pub time: u32,
}

impl<'a> From<&'a Event> for &'a EndTurnStart {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndTurnStart(event) => event,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct EndTurnEnd {
    pub count: u32,
    pub valid: u32,
    pub time: u32,
    #[cfg_attr(feature = "serde", serde(rename = "isvalid"))]
    pub is_valid: bool,
}

impl<'a> From<&'a Event> for &'a EndTurnEnd {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::EndTurnEnd(event) => event,
            _ => unreachable!(),
        }
    }
}
