#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct EphemeralState {
    restarted: u16,
    warmup: bool,
}

impl EphemeralState {
    pub fn new() -> Self {
        Self {
            restarted: 0,
            warmup: false,
        }
    }
}
