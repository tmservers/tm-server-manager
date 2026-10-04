use spacetimedb::{ReducerContext, SpacetimeType, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, node::NodeHandle},
    leaderboard::LbEntry,
    tm_match::{state::tab_match_state__view, tab_match},
};

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(super) enum PlayerAction {
    StartLine(u32),
    Checkpoint(PlayerActionCheckpoint),
    Respawn(PlayerActionRespawn),
    GiveUp(u32),
    Lap(PlayerActionCheckpoint),
    Finish(PlayerActionCheckpoint),
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(super) struct PlayerActionRespawn {
    time: u32,
    speed: f32,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(super) struct PlayerActionCheckpoint {
    speed: f32,
    time: u32,
}

// TODO change back to private
#[table(accessor= tab_match_round_player,
    index(accessor=match_round, hash(columns=[match_id,round])),
    index(accessor=match_round_range, btree(columns=[match_id,round])),
    index(accessor=match_round_player, hash(columns=[match_id,round,user_id])),
    public
)]
#[derive(Debug, Clone, Copy)]
pub struct MatchRoundPlayer {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    pub user_id: u32,

    #[index(hash)]
    match_id: u32,
    time: i32,
    // The points of the round.
    points: i32,

    round: u16,
    #[default(0)]
    position: u16,
}

impl MatchRoundPlayer {
    pub(super) fn new(match_id: u32, user_id: u32, round: u16) -> Self {
        Self {
            user_id,
            match_id,
            round,
            time: 0,
            points: 0,
            id: 0,
            position: 0,
        }
    }

    pub(super) fn set_time(&mut self, points: i32) {
        self.time = points;
    }

    pub(super) fn set_points(&mut self, points: i32) {
        self.points = points;
    }

    pub(super) fn set_position(&mut self, position: u16) {
        self.position = position;
    }

    pub(crate) fn get_time(&self) -> i32 {
        self.time
    }

    pub(crate) fn get_round(&self) -> u16 {
        self.round
    }

    pub(crate) fn get_position(&self) -> u16 {
        self.position
    }

    pub(crate) fn get_score(&self) -> i32 {
        self.points
    }
}

//TODO change back to private
#[table(accessor= tab_match_round_player_ext,
    index(accessor=match_round, hash(columns=[match_id,round])),
    index(accessor=match_round_range, btree(columns=[match_id,round])),
    index(accessor=match_round_player, hash(columns=[match_id,round,user_id])),
    public
)]
pub struct MatchRoundPlayerExt {
    round_actions: Vec<PlayerAction>,

    user_id: u32,
    #[primary_key]
    pub id: u32,
    #[index(hash)]
    match_id: u32,
    round: u16,
}

impl MatchRoundPlayerExt {
    pub fn new(id: u32, match_id: u32, user_id: u32, round: u16, server_time: u32) -> Self {
        Self {
            user_id,
            match_id,
            round,
            round_actions: vec![PlayerAction::StartLine(server_time)],
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
    pub(crate) fn add_lap(&mut self, speed: f32, time: u32) {
        self.round_actions
            .push(PlayerAction::Lap(PlayerActionCheckpoint { speed, time }));
    }
    pub(crate) fn add_finish(&mut self, speed: f32, time: u32) {
        self.round_actions
            .push(PlayerAction::Finish(PlayerActionCheckpoint { speed, time }));
    }
    pub(crate) fn add_start_line(&mut self, server_time: u32) {
        self.round_actions
            .push(PlayerAction::StartLine(server_time));
    }

    pub(crate) fn add_respawn(&mut self, speed: f32, server_time: u32) {
        let first = *self.round_actions.first().unwrap();

        // Double respawn.
        if speed == 0.
            && let Some(last) = self.round_actions.last_mut()
            && let PlayerAction::Respawn(respawn) = last
        {
            respawn.speed = speed;

            return;
        };

        if let PlayerAction::StartLine(time) = first {
            self.round_actions
                .push(PlayerAction::Respawn(PlayerActionRespawn {
                    speed,
                    time: server_time - time,
                }));
        } else {
            log::error!("First event in a RoundAction was something other than start line event.")
        }
    }

    pub(crate) fn give_up(&mut self, server_time: u32) {
        let first = self.round_actions.first().unwrap();

        if let PlayerAction::StartLine(time) = *first {
            self.round_actions
                .push(PlayerAction::GiveUp(server_time - time));
        } else {
            log::error!("First event in a RoundAction was something other than start line event.")
        }
    }
}

pub(crate) trait MatchLeadearboardRead {
    //fn match_leaderboard(&self, match_id: u32, round: u16) -> Vec<MatchRoundPlayer>;
    fn match_rounds(&self, match_id: u32) -> Vec<LbEntry>;
    //fn match_leaderboard(&self, match_id: u32) -> Vec<LbEntry>;
}
impl<Db: spacetimedb::CtxDbRead> MatchLeadearboardRead for Db {
    fn match_rounds(&self, match_id: u32) -> Vec<LbEntry> {
        let Some(state) = self
            .db_read_only()
            .tab_match_state()
            .match_id()
            .find(match_id)
        else {
            return Vec::new();
        };
        self.db_read_only()
            .tab_match_round_player()
            .match_id()
            .filter(match_id)
            .map(|f| {
                LbEntry::new(
                    f.user_id,
                    state.get_mode(),
                    f.position,
                    NodeHandle::MatchV1(match_id),
                )
                .set_score(f.points)
                .set_time(f.time)
                .set_round(f.round)
                //.set_map(/* map */) //TODO get the map id in there. this is pretty painful tho so defer until i know approach is good.
            })
            .collect()
    }
}

#[reducer]
fn match_round_player_set_score_manual(
    ctx: &ReducerContext,
    match_id: u32,
    user_id: u32,
    round: u16,
    score: i32,
) -> Result<(), String> {
    let Some(tm_match) = ctx.db.tab_match().id().find(match_id) else {
        return Err("Match not found!".into());
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::OWNER)
        .authorize()?;

    let Some(mut entry) = ctx
        .db
        .tab_match_round_player()
        .match_round_player()
        .filter((match_id, round, user_id))
        .next()
    else {
        return Err("No entry was found.".into());
    };

    entry.points = score;

    ctx.db.tab_match_round_player().id().update(entry);

    Ok(())
}

#[reducer]
fn match_round_player_set_position_manual(
    ctx: &ReducerContext,
    match_id: u32,
    user_id: u32,
    round: u16,
    position: u16,
) -> Result<(), String> {
    let Some(tm_match) = ctx.db.tab_match().id().find(match_id) else {
        return Err("Match not found!".into());
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::OWNER)
        .authorize()?;

    let Some(mut entry) = ctx
        .db
        .tab_match_round_player()
        .match_round_player()
        .filter((match_id, round, user_id))
        .next()
    else {
        return Err("No entry was found.".into());
    };

    entry.position = position;

    ctx.db.tab_match_round_player().id().update(entry);

    Ok(())
}
