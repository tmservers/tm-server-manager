/// The pause of the modescript, `Maniaplanet.Pause.Status`. The script sends it as
/// the answer to `Maniaplanet.Pause.GetStatus` and `Maniaplanet.Pause.SetActive`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Pause {
    /// The mode can pause.
    pub available: bool,
    pub active: bool,
}

impl Pause {
    pub const CALLBACK: &str = "Maniaplanet.Pause.Status";
}
