#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct StartLine {
    #[cfg_attr(feature = "serde", serde(rename = "accountid"))]
    pub account_id: String,
    pub login: String,
    pub time: u32,
}
