use std::collections::HashMap;

use spacetimedb::{AnonymousViewContext, CtxWithTimestamp, Table, Timestamp, table, view};
use tm_server_types::config::TmMode;

use crate::{
    maps::{TabTmMap, TmMapRead},
    tm_match::MatchStatus,
};

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

#[cfg(test)]
mod tests {
    use spacetimedb::{
        TimeDuration,
        test_utils::{TestAuth, TestContext},
    };

    use super::*;
    use crate::pipeline::test_util::test_context;

    fn history(test: &TestContext) -> Vec<MatchStateHistory> {
        let mut history: Vec<MatchStateHistory> = test
            .db
            .tab_match_state_history()
            .match_id()
            .filter(1u32)
            .collect();
        history.sort_by_key(|h| h.at);
        history
    }

    /// Applies the changes to match 1 in a transaction of their own.
    fn transaction(test: &TestContext, changes: &[fn(&mut MatchState)]) {
        test.clock.advance(TimeDuration::from_micros(1));
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            for change in changes {
                let mut state = ctx.match_state_find(1).unwrap();
                change(&mut state);
                ctx.match_state_update(state);
            }
            Ok::<_, String>(())
        })
        .unwrap();
    }

    #[test]
    fn a_transaction_only_keeps_the_state_it_ended_with() {
        let test = test_context(&[(1, TmMode::Rounds)]);

        transaction(&test, &[|s| s.set_map(7), |s| s.new_round()]);

        // The state the match started with and the one after the transaction.
        let rows: Vec<(u16, u32)> = history(&test).iter().map(|h| (h.round, h.map_id)).collect();
        assert_eq!(rows, vec![(0, 0), (1, 7)]);
    }

    #[test]
    fn an_unchanged_state_is_not_added_again() {
        let test = test_context(&[(1, TmMode::Rounds)]);

        transaction(&test, &[|_| ()]);

        assert_eq!(history(&test).len(), 1);
    }

    #[test]
    fn rounds_after_a_restart_are_played_on_the_maps_of_the_new_attempt() {
        let test = test_context(&[(1, TmMode::Rounds)]);
        transaction(&test, &[|s| s.set_map(7)]);
        transaction(&test, &[|s| s.new_round()]);
        transaction(&test, &[|s| s.restart()]);
        transaction(&test, &[|s| s.set_map(8)]);
        transaction(&test, &[|s| s.new_round()]);

        let cx = test.anonymous_view_context();
        let maps = cx.match_round_maps(&cx.match_state_find(1).unwrap());

        assert_eq!(maps.get(&1), Some(&8));
    }
}
