use std::collections::HashMap;

use spacetimedb::{AnonymousViewContext, SpacetimeType, Uuid, table, view};

use crate::{tm_match::state::tab_match_state__view, user::tab_user__view};

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(super) enum PlayerAction {
    GiveUp,
    Respawn(PlayerActionRespawn),
    Checkpoint(PlayerActionCheckpoint),
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(super) struct PlayerActionRespawn {
    speed: f32,
    standing: bool,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(super) struct PlayerActionCheckpoint {
    speed: f32,
    time: u32,
}

#[table(accessor= tab_match_round_player,
    index(accessor=match_round, hash(columns=[match_id,round])),
    index(accessor=match_round_range, btree(columns=[match_id,round])),
    index(accessor=match_round_player, hash(columns=[match_id,round,user_id]))
)]
pub struct TabMatchRoundPlayer {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    user_id: u32,

    match_id: u32,
    time: i32,
    // The points of the round.
    points: i32,

    round: u16,
}

impl TabMatchRoundPlayer {
    pub fn new(match_id: u32, user_id: u32, round: u16) -> Self {
        Self {
            user_id,
            match_id,
            round,
            time: 0,
            points: 0,
            id: 0,
        }
    }
}

#[table(accessor= tab_match_round_player_ext,
    index(accessor=match_round, hash(columns=[match_id,round])),
    index(accessor=match_round_range, btree(columns=[match_id,round])),
    index(accessor=match_round_player, hash(columns=[match_id,round,user_id]))
)]
pub struct TabMatchRoundPlayerExt {
    round_actions: Vec<PlayerAction>,

    user_id: u32,
    match_id: u32,
    #[primary_key]
    pub id: u32,
    round: u16,
}

impl TabMatchRoundPlayerExt {
    pub fn new(id: u32, match_id: u32, user_id: u32, round: u16) -> Self {
        Self {
            user_id,
            match_id,
            round,
            round_actions: Vec::new(),
            id,
        }
    }

    pub(crate) fn add_checkpoint(&mut self, speed: f32, time: u32) {
        self.round_actions
            .push(PlayerAction::Checkpoint(PlayerActionCheckpoint {
                speed,
                time,
            }));
    }

    pub(crate) fn add_respawn(&mut self, speed: f32) {
        self.round_actions
            .push(PlayerAction::Respawn(PlayerActionRespawn {
                speed,
                standing: speed == 0.,
            }));
    }

    pub(crate) fn give_up(&mut self) {
        self.round_actions.push(PlayerAction::GiveUp);
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub struct MatchRoundPlayer {
    pub account_id: Uuid,

    id: u32,
    // We can most likely omit this match_id because we already query after the match so it should be obvious.
    // For now its not really an issue but maybe this can be replaced with something else.
    match_id: u32,
    // We can most likely omit this. maybe we could include the best match round time? -> then we should rename.
    time: i32,
    // The points of the round.
    score: i32,

    round: u16,
    position: u16,
}

#[derive(Debug, SpacetimeType, Clone)]
pub struct MatchRoundPlayerExt {
    round_actions: Vec<PlayerAction>,
    id: u32,
    match_id: u32,
    round: u16,
}

/// Accumulates points of all previous rounds.
/// Round 0 is giving you a live view.
/// If you want points from individual rounds use the match_round view instead.
#[view(accessor=match_leaderboard,public)]
pub fn match_leaderboard(
    ctx: &AnonymousViewContext, /*, match_id: u32, round: u16 */
) -> Vec<MatchRoundPlayer> {
    let match_id = 1u32;
    let mut round = 0u16;
    let entries: Vec<TabMatchRoundPlayer> = if round == 0 {
        let Some(state) = ctx.db.tab_match_state().match_id().find(match_id) else {
            return Vec::new();
        };
        round = state.get_round();
        ctx.db
            .tab_match_round_player()
            .match_round_range()
            .filter((match_id, 1..round))
            .collect()
    } else {
        ctx.db
            .tab_match_round_player()
            .match_round_range()
            .filter((match_id, 1..=round))
            .collect()
    };

    let mut map = HashMap::<u32, TabMatchRoundPlayer>::new();

    for entry in entries {
        map.entry(entry.user_id)
            .and_modify(|e| {
                e.points += entry.points;
                if entry.round > e.round {
                    e.round = entry.round
                }
            })
            .or_insert(entry);
    }

    let mut standings = map
        .into_values()
        .map(|p| MatchRoundPlayer {
            account_id: ctx
                .db
                .tab_user()
                .internal_id()
                .find(p.user_id)
                .unwrap()
                .account_id,
            id: p.id,
            match_id,
            time: p.time,
            score: p.points,
            round: p.round,
            position: 0,
        })
        .collect::<Vec<_>>();

    standings.sort_by_key(|v| v.score);
    for (position, entry) in standings.iter_mut().enumerate() {
        entry.position = (position + 1) as u16;
    }
    standings
}

/// Returns the specified round of the match.
/// Round 0 is giving you a live view.
/// If you want a accumulated view please you the match_leaderboard view instead.
#[view(accessor=match_round,public)]
pub fn match_round(
    ctx: &AnonymousViewContext, /*, match_id: u32, round: u16 */
) -> Vec<MatchRoundPlayer> {
    let match_id = 1u32;
    let mut round = 0u16;

    if round == 0 {
        let Some(state) = ctx.db.tab_match_state().match_id().find(match_id) else {
            return Vec::new();
        };
        round = state.get_round();
    }

    let mut standings = ctx
        .db
        .tab_match_round_player()
        .match_round()
        .filter((match_id, round))
        .map(|p| MatchRoundPlayer {
            account_id: ctx
                .db
                .tab_user()
                .internal_id()
                .find(p.user_id)
                .unwrap()
                .account_id,
            match_id,
            id: p.id,
            time: p.time,
            score: p.points,
            round: p.round,
            position: 0,
        })
        .collect::<Vec<_>>();
    standings.sort_by_key(|v| v.score);
    for (position, entry) in standings.iter_mut().enumerate() {
        entry.position = (position + 1) as u16;
    }
    standings
}

/// If round 0 is supplied we take the current round.
#[view(accessor=match_round_ext,public)]
pub fn match_round_ext(
    ctx: &AnonymousViewContext, /* match_id: u32, round: u16 */
) -> Vec<MatchRoundPlayerExt> {
    let match_id = 1u32;
    let mut round = 0u16;

    if round == 0 {
        let Some(state) = ctx.db.tab_match_state().match_id().find(match_id) else {
            return Vec::new();
        };
        round = state.get_round();
    }

    ctx.db
        .tab_match_round_player_ext()
        .match_round()
        .filter((match_id, round))
        .map(|p| MatchRoundPlayerExt {
            round_actions: p.round_actions,
            id: p.id,
            match_id,
            round,
        })
        .collect()
}
