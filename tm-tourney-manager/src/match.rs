use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::{
    r#match::leaderboard::Leaderboard,
    server::{TmServer, tm_server},
    stage::{self, event_stage},
};

mod leaderboard;

// The table name needs to be plural since match is a rust keyword
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = stage_match, public))]
pub struct StageMatch {
    #[auto_inc]
    #[primary_key]
    pub id: u64,

    /// The stage this match is associated with.
    stage_id: u64,
    /// The assigned server that will be used by this match.
    server_id: Option<String>,

    /// The moment the server is assigned to the match the pre_match_config gets loaded in.
    /// Only if it is defined. Useful for hiding tournament maps till the actual start.
    pre_match_config: Option<ServerConfig>,
    /// If the match is started this config gets loaded.
    /// Has to be specified before your able to advance into Upcoming.
    match_config: Option<ServerConfig>,
    post_match_config: Option<ServerConfig>,

    status: MatchStatus,
    leaderboard: Leaderboard,
}

impl StageMatch {
    /// Evaluates is the Match is in the "Match" state of its lifecycle.
    pub fn is_live(&self) -> bool {
        self.status == MatchStatus::Live
    }

    pub fn add_server_event(&mut self, event: &Event) {
        // Not worth defining as an invariant for calling so need to be sure.
        if !self.is_live() {
            return;
        }

        match event {
            Event::WayPoint(way_point) => todo!(),
            Event::Respawn(respawn) => todo!(),
            Event::StartLine(start_line) => todo!(),
            Event::Scores(scores) => todo!(),
            Event::GiveUp(give_up) => todo!(),
            Event::LoadingMapStart(loading_map_start) => todo!(),
            Event::LoadingMapEnd(loading_map_end) => todo!(),
            Event::StartMapStart(start_map) => todo!(),
            Event::StartMapEnd(start_map) => todo!(),
            Event::EndMapStart(end_map_start) => todo!(),
            Event::EndMapEnd(end_map_end) => todo!(),
            Event::UnloadingMapStart(unloading_map_start) => todo!(),
            Event::UnloadingMapEnd(unloading_map_end) => todo!(),
            Event::PlayerConenct(player_connect) => todo!(),
            Event::PlayerDisconnect(player_disconnect) => todo!(),
            Event::PlayerChat(player_chat) => todo!(),
            Event::StartTurnStart(start_turn) => todo!(),
            Event::StartTurnEnd(start_turn) => todo!(),
            Event::PlayLoopStart(play_loop_start) => todo!(),
            Event::PlayLoopEnd(play_loop_end) => todo!(),
            Event::EndRoundStart(end_round_start) => todo!(),
            Event::EndRoundEnd(end_round_end) => todo!(),
            Event::PodiumStart(podium) => todo!(),
            Event::PodiumEnd(podium) => todo!(),
            Event::Custom(custom) => todo!(),
            _ => todo!(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum MatchStatus {
    Configuring,
    Upcoming,
    PreMatch,
    Live,
    PostMatch,
    Ended,
}

/// Provisions a new StageMatch to the specified EventStage and a MatchTemplate.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn provision_match(
    ctx: &ReducerContext,
    used_by: u64,
    with_config: Option<u64>,
    auto_provisioning_server: bool,
) {
    //TODO authorization
    if let Some(mut stage) = ctx.db.event_stage().id().find(used_by) {
        let stage_match = ctx.db.stage_match().insert(StageMatch {
            id: 0,
            stage_id: used_by,
            status: MatchStatus::Configuring,
            server_id: if auto_provisioning_server { None } else { None },
            pre_match_config: None,
            match_config: None,
            post_match_config: None,
            leaderboard: Leaderboard::new(),
        });
        stage.add_match(stage_match.id);

        ctx.db.event_stage().id().update(stage);
    }
}

/// Assigns a server to the selected match.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_assign_server(ctx: &ReducerContext, to: u64, server_id: String) {
    //TODO authorization
    if let Some(mut server) = ctx.db.tm_server().id().find(&server_id)
        && server.active_match().is_none()
        && let Some(stage_match) = ctx.db.stage_match().id().find(to)
        && stage_match.status == MatchStatus::Configuring
    {
        let stage_match = ctx.db.stage_match().id().update(StageMatch {
            server_id: Some(server_id),
            ..stage_match
        });

        server.set_active_match(stage_match.id);

        ctx.db.tm_server().id().update(server);
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_configured(ctx: &ReducerContext, id: u64) {
    //TODO authorization
    if let Some(mut stage_match) = ctx.db.stage_match().id().find(id)
        && stage_match.status == MatchStatus::Configuring
    {
        stage_match.status = MatchStatus::Upcoming;
        ctx.db.stage_match().id().update(stage_match);
    }
}

/* #[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn update_pre_match_config(ctx: &ReducerContext, id: u64, config: ServerConfig) {
    //TODO authorization
    if let Some(mut stage_match) = ctx.db.stage_match().id().find(id) {
        stage_match.match_config = Some(config);
        ctx.db.stage_match().id().update(stage_match);
    }
} */

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn update_match_config(ctx: &ReducerContext, id: u64, config: ServerConfig) {
    //TODO authorization
    if let Some(mut stage_match) = ctx.db.stage_match().id().find(id) {
        stage_match.match_config = Some(config);
        ctx.db.stage_match().id().update(stage_match);
    }
}

/// If the match is fully configured and ready start.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn try_start(ctx: &ReducerContext, match_id: u64) {
    //TODO authorization
    if let Some(mut stage_match) = ctx.db.stage_match().id().find(match_id)
        && let Some(server) = &stage_match.server_id
        && let Some(mut server) = ctx.db.tm_server().id().find(server)
        && let Some(config) = &stage_match.match_config
        && stage_match.status == MatchStatus::Upcoming
    {
        server.set_config(config.clone());
        stage_match.status = MatchStatus::Live;
        ctx.db.stage_match().id().update(stage_match);
        ctx.db.tm_server().id().update(server);
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = match_template,public))]
pub struct MatchTemplate {
    #[auto_inc]
    #[primary_key]
    id: u64,

    creator: String,
}
