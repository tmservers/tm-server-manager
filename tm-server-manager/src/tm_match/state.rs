use spacetimedb::{AnonymousViewContext, Query, Uuid, table, view};

#[derive(Debug, Copy, Clone)]
#[table(accessor=tab_match_state)]
pub struct MatchState {
    map_id: Uuid,
    #[primary_key]
    pub(crate) match_id: u32,
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
            map_id: Uuid::NIL,
        }
    }

    pub(crate) fn set_wu(&mut self, active: bool) {
        self.is_warmup = active;
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
}

#[view(accessor=match_state,public)]
pub fn match_state(ctx: &AnonymousViewContext /* match_id:u32 */) -> impl Query<MatchState> {
    let match_id = 1u32;
    ctx.from
        .tab_match_state()
        .r#where(|c| c.match_id.eq(match_id))
}
