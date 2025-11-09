#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct MatchState {
    restarted: u16,
    round: u16,
    warmup: u16,
    is_warmup: bool,
    paused: bool,
    // map_uid: String,
}

impl MatchState {
    pub fn new() -> Self {
        Self {
            restarted: 0,
            round: 0,
            warmup: 0,
            is_warmup: false,
            paused: false,
            //map_uid: "".into(),
        }
    }

    pub(crate) fn enable_wu(&mut self) {
        self.is_warmup = true;
    }

    pub(crate) fn disable_wu(&mut self) {
        self.is_warmup = false;
    }

    pub(crate) fn new_wu_round(&mut self) {
        self.warmup += 1;
    }

    pub(crate) fn new_round(&mut self) {
        self.round += 1;
    }
}
