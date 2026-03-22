use spacetimedb::{AnonymousViewContext, Query, Uuid, table, view};

#[derive(Debug, Copy, Clone)]
#[table(accessor=tab_match_state)]
pub struct MatchState {
    #[primary_key]
    pub(crate) match_id: u32,
    map_id: u32,
    restarted: u16,
    round: u16,
    warmup: u16,
    is_warmup: bool,
    paused: bool,
}

impl MatchState {
    pub fn new(match_id: u32) -> Self {
        Self {
            match_id,
            restarted: 0,
            round: 0,
            warmup: 0,
            is_warmup: false,
            paused: false,
            map_id: 0,
        }
    }

    pub(crate) fn set_wu(&mut self, active: bool) {
        self.is_warmup = active;
    }

    pub(crate) fn set_map(&mut self, id: u32) {
        self.map_id = id;
    }

    pub(crate) fn set_pause(&mut self, paused: bool) {
        self.paused = paused;
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

    pub(super) fn live_round(&self) -> bool {
        !self.is_warmup && !self.paused
    }
}

#[view(accessor=match_state,public)]
pub fn match_state(ctx: &AnonymousViewContext /* match_id:u32 */) -> Option<MatchState> {
    let match_id = 51u32;
    //TODO map the internal map id to string.
    ctx.db.tab_match_state().match_id().find(match_id)
    //.map(|a| a)
}
