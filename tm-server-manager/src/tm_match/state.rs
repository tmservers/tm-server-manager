use std::collections::HashMap;

use spacetimedb::{AnonymousViewContext, CtxWithTimestamp, Table, Timestamp, table, view};
use tm_server_types::config::TmMode;

use crate::tm_match::MatchStatus;

#[derive(Debug, Copy, Clone, PartialEq)]
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
        self.restarted = self.restarted.saturating_add(1);
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

    pub(crate) fn get_mode(&self) -> TmMode {
        self.mode
    }
}

/// Every state a match was in, so what happened in it can be replayed.
/// There is one row for every transaction which changed the state, the one it had at the end of it.
#[derive(Debug, Copy, Clone, PartialEq)]
#[table(accessor=tab_match_state_history, vis_private,
    index(accessor=match_at, hash(columns=[match_id, at])))]
pub struct MatchStateHistory {
    #[index(hash)]
    match_id: u32,
    /// When the transaction changing the state ran.
    at: Timestamp,
    map_id: u32,
    restarted: u16,
    round: u16,
    warmup: u16,
    is_warmup: bool,
    paused: bool,
    status: MatchStatus,
    mode: TmMode,
}

impl MatchStateHistory {
    fn new(state: &MatchState, at: Timestamp) -> Self {
        // Without `..` a new field of the state does not compile until it is part of the history too.
        let MatchState {
            match_id,
            map_id,
            restarted,
            round,
            warmup,
            is_warmup,
            paused,
            status,
            mode,
        } = *state;
        MatchStateHistory {
            match_id,
            at,
            map_id,
            restarted,
            round,
            warmup,
            is_warmup,
            paused,
            status,
            mode,
        }
    }
}

#[view(accessor=match_state,public)]
pub fn match_state(ctx: &AnonymousViewContext /* match_id:u32 */) -> Option<MatchState> {
    let match_id = 51u32;
    //TODO map the internal map id to string.
    ctx.db.tab_match_state().match_id().find(match_id)
    //.map(|a| a)
}

pub(crate) trait MatchStateRead {
    fn match_state_find(&self, match_id: u32) -> Option<MatchState>;
    /// The map every round since the last restart of the match was played on.
    fn match_round_maps(&self, state: &MatchState) -> HashMap<u16, u32>;
}
impl<Db: spacetimedb::CtxDbRead> MatchStateRead for Db {
    fn match_state_find(&self, match_id: u32) -> Option<MatchState> {
        self.db_read_only()
            .tab_match_state()
            .match_id()
            .find(match_id)
    }

    fn match_round_maps(&self, state: &MatchState) -> HashMap<u16, u32> {
        let mut history: Vec<MatchStateHistory> = self
            .db_read_only()
            .tab_match_state_history()
            .match_id()
            .filter(state.match_id)
            .filter(|h| h.restarted == state.restarted)
            .collect();
        history.sort_by_key(|h| h.at);

        let mut maps = HashMap::new();
        for h in history {
            // A round is played on the map it was started on, the next map gets loaded before the next round starts.
            maps.entry(h.round).or_insert(h.map_id);
        }
        maps
    }
}

pub(crate) trait MatchStateWrite: MatchStateRead {
    fn match_state_insert(&self, state: MatchState) -> Result<(), String>;
    fn match_state_update(&self, state: MatchState);
}
impl<Db: spacetimedb::CtxDbWrite + CtxWithTimestamp> MatchStateWrite for Db {
    fn match_state_insert(&self, state: MatchState) -> Result<(), String> {
        self.db().tab_match_state().try_insert(state)?;
        self.db()
            .tab_match_state_history()
            .insert(MatchStateHistory::new(&state, self.timestamp()));
        Ok(())
    }

    fn match_state_update(&self, state: MatchState) {
        let previous = self.match_state_find(state.match_id);
        if previous == Some(state) {
            return;
        }
        self.db().tab_match_state().match_id().update(state);

        let at = self.timestamp();
        let history = self.db().tab_match_state_history();
        // Only the state at the end of a transaction is ever seen, so an earlier change in the same one gets replaced.
        history.match_at().delete((state.match_id, at));
        history.insert(MatchStateHistory::new(&state, at));
    }
}
