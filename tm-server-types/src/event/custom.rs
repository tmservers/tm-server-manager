#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Custom {
    pub name: String,
    pub body: String,
}

impl Custom {
    pub(crate) fn new(name: String, body: String) -> Self {
        Custom { name, body }
    }
}
