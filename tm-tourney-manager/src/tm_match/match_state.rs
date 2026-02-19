use spacetimedb::table;
use tm_server_types::event::Event;

#[derive(Debug, Clone, Copy, Default)]
#[table(name=tab_tm_match_state)]
pub struct TmMatchState {
    #[primary_key]
    pub(crate) id: u32,
    pub(crate) restarted: u16,
    pub(crate) round: u16,
    pub(crate) warmup: u16,
    pub(crate) is_warmup: bool,
    pub(crate) paused: bool,
    // map_uid: String,
}

impl TmMatchState {
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

    /// # Safety:
    /// Must only be called if match is live.
    pub fn add_server_event(&mut self, event: &Event) -> bool {
        //TODO
        match event {
            Event::WarmupStart => self.enable_wu(),
            Event::WarmupEnd => self.disable_wu(),
            Event::WarmupStartRound(_) => self.new_wu_round(),
            Event::StartRoundStart(_) => self.new_round(),
            _ => return false,
        }
        log::warn!("{:#?}", self);
        true
    }
}
