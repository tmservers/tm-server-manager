use spacetimedb::{AnonymousViewContext, Table, table, view};
use tm_server_types::config::TmMode;

use crate::{
    maps::{TabTmMap, TmMapRead},
    tm_match::MatchStatus,
};

#[derive(Debug, Copy, Clone)]
#[table(accessor=tab_match_state, vis_private)]
pub struct MatchState {
    #[primary_key]
    pub(crate) match_id: u32,
    map_id: u32,
    restarted: u16,
    round: u16,
    warmup: u16,
    is_warmup: bool,
    paused: bool,
    status: MatchStatus,
    // This would be free in terms of bytes.
    // Could be nice to evaluate score for special modes like time attack
    // or Knockout.
    #[default(TmMode::Rounds)]
    mode: TmMode,
}

impl MatchState {
    pub fn new(match_id: u32, mode: TmMode) -> Self {
        Self {
            match_id,
            restarted: 0,
            round: 0,
            warmup: 0,
            is_warmup: false,
            paused: false,
            map_id: 0,
            status: MatchStatus::Preparation,
            mode,
        }
    }

    pub(crate) fn set_wu(&mut self, active: bool) {
        self.is_warmup = active;
    }

    pub(crate) fn is_live(&self) -> bool {
        matches!(self.status, MatchStatus::Live) || matches!(self.status, MatchStatus::LiveComitted)
    }

    pub(crate) fn set_map(&mut self, id: u32) {
        self.map_id = id;
    }

    pub(crate) fn set_pause(&mut self, paused: bool) {
        if !self.paused && paused {
            self.round -= 1;
        }
        self.paused = paused;
    }

    pub(crate) fn new_wu_round(&mut self) {
        self.warmup += 1;
    }

    pub(crate) fn new_round(&mut self) {
        self.round += 1;
    }

    pub(crate) fn set_live(&mut self) {
        self.status = MatchStatus::Live;
    }

    pub(crate) fn set_live_commited(&mut self) {
        self.status = MatchStatus::LiveComitted;
    }

    pub(crate) fn end_match(&mut self) {
        self.status = MatchStatus::Ended;
    }

    pub(crate) fn set_recovery(&mut self) {
        self.status = MatchStatus::Recovery;
    }

    pub(crate) fn restart(&mut self) {
        self.restarted = 0;
        self.round = 0;
        self.warmup = 0;
        self.is_warmup = false;
        self.paused = false;
    }

    pub(super) fn get_round(&self) -> u16 {
        self.round
    }

    pub(super) fn get_map(&self) -> u32 {
        self.map_id
    }

    pub(super) fn paused(&self) -> bool {
        self.paused
    }

    pub(crate) fn status(&self) -> MatchStatus {
        self.status
    }

    pub(super) fn live_round(&self) -> bool {
        !self.is_warmup && !self.paused
    }

    pub(super) fn is_live_commited(&self) -> bool {
        matches!(self.status, MatchStatus::LiveComitted)
    }

    pub(super) fn get_mode(&self) -> TmMode {
        self.mode
    }
}

#[view(accessor=match_state,public)]
pub fn match_state(ctx: &AnonymousViewContext, match_id: u32) -> Option<MatchState> {
    ctx.db.tab_match_state().match_id().find(match_id)
}

/// The map currently played in a match.
#[view(accessor=match_map,public)]
pub fn match_map(ctx: &AnonymousViewContext, match_id: u32) -> Option<TabTmMap> {
    ctx.tm_map_find(ctx.match_state_find(match_id)?.map_id)
}

pub(crate) trait MatchStateRead {
    fn match_state_find(&self, match_id: u32) -> Option<MatchState>;
}
impl<Db: spacetimedb::CtxDbRead> MatchStateRead for Db {
    fn match_state_find(&self, match_id: u32) -> Option<MatchState> {
        self.db_read_only()
            .tab_match_state()
            .match_id()
            .find(match_id)
    }
}

pub(crate) trait MatchStateWrite: MatchStateRead {
    fn match_state_insert(&self, state: MatchState) -> Result<(), String>;
    fn match_state_update(&self, state: MatchState);
}
impl<Db: spacetimedb::CtxDbWrite> MatchStateWrite for Db {
    fn match_state_insert(&self, state: MatchState) -> Result<(), String> {
        self.db().tab_match_state().try_insert(state)?;
        Ok(())
    }

    fn match_state_update(&self, state: MatchState) {
        self.db().tab_match_state().match_id().update(state);
    }
}
