use spacetimedb::{AnonymousViewContext, SpacetimeType, Uuid, table, view};

use crate::tm_match::state::tab_tm_match_state__view;

#[derive(Debug, SpacetimeType)]
pub(super) enum PlayerAction {
    GiveUp,
    Respawn(PlayerActionRespawn),
    Checkpoint(PlayerActionCheckpoint),
}

#[derive(Debug, SpacetimeType)]
pub(super) struct PlayerActionRespawn {
    speed: f32,
    standing: bool,
}

#[derive(Debug, SpacetimeType)]
pub(super) struct PlayerActionCheckpoint {
    speed: f32,
    time: u32,
}

#[table(accessor= tab_tm_match_round_player,
    index(accessor=match_round, hash(columns=[match_id,round])),
    index(accessor=match_round_player, hash(columns=[match_id,round,internal_account_id]))
)]
pub struct TmMatchRoundPlayer {
    internal_account_id: u32,

    match_id: u32,
    time: i32,
    round_points: i32,

    round: u16,
    // maybe accumulate this in the view.
    // match_points: i32,
}

impl TmMatchRoundPlayer {
    pub fn new(match_id: u32, internal_account_id: u32, round: u16) -> Self {
        Self {
            internal_account_id,
            match_id,
            round,
            time: 0,
            round_points: 0,
            //match_points: 0,
        }
    }
}

#[table(accessor= tab_tm_match_round_player_ext,
    index(accessor=match_round, hash(columns=[match_id,round])),
    index(accessor=match_round_player, hash(columns=[match_id,round,internal_account_id]))
)]
pub struct TmMatchRoundPlayerExt {
    round_actions: Vec<PlayerAction>,

    internal_account_id: u32,
    match_id: u32,
    round: u16,

    #[auto_inc]
    #[primary_key]
    pub id: u32,
}

impl TmMatchRoundPlayerExt {
    pub fn new(match_id: u32, internal_account_id: u32, round: u16) -> Self {
        Self {
            internal_account_id,
            match_id,
            round,
            round_actions: Vec::new(),
            id: 0,
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

/// Returns the specified round of the match.
/// Round 0 is giving you a live view.
#[view(accessor=match_round,public)]
pub fn match_round(
    ctx: &AnonymousViewContext, /*, match_id: u32, round: u16 */
) -> Vec<TmMatchRoundPlayer> {
    let match_id = 1u32;
    let mut round = 0u16;

    if round == 0 {
        let Some(state) = ctx.db.tab_tm_match_state().match_id().find(match_id) else {
            return Vec::new();
        };
        round = state.get_round();
    }

    ctx.db
        .tab_tm_match_round_player()
        .match_round()
        .filter((match_id, round))
        .collect()
}

/// If round 0 is supplied we take the current round.
#[view(accessor=match_round_ext,public)]
pub fn match_round_ext(
    ctx: &AnonymousViewContext, /* match_id: u32, round: u16 */
) -> Vec<TmMatchRoundPlayerExt> {
    let match_id = 1u32;
    let mut round = 0u16;

    if round == 0 {
        let Some(state) = ctx.db.tab_tm_match_state().match_id().find(match_id) else {
            return Vec::new();
        };
        round = state.get_round();
    }

    ctx.db
        .tab_tm_match_round_player_ext()
        .match_round()
        .filter((match_id, round))
        .collect()
}
