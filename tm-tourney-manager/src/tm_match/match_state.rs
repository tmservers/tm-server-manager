use spacetimedb::{Uuid, table};
use tm_server_types::event::Event;

#[derive(Debug, Copy, Clone)]
#[table(accessor=tab_tm_match_state)]
pub struct TmMatchState {
    #[primary_key]
    pub(crate) match_id: u32,
    restarted: u16,
    round: u16,
    warmup: u16,
    is_warmup: bool,
    paused: bool,
    map_id: Uuid,
}

impl TmMatchState {
    pub fn new(match_id: u32) -> Self {
        Self {
            match_id,
            restarted: 0,
            round: 0,
            warmup: 0,
            is_warmup: false,
            paused: false,
            map_id: Uuid::NIL,
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

    pub(super) fn get_round(&self) -> u16 {
        self.round
    }
}
