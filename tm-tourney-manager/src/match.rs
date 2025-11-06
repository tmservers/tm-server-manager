use spacetimedb::{DbContext, ReducerContext, SpacetimeType, Table, reducer, table};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::{
    auth::Authorization,
    competition::competition,
    r#match::{ephemeral_state::EphemeralState, leaderboard::MatchLeaderboardRules},
    registration::Registration,
    scheduling::Scheduling,
    server::{TmServer, tm_server},
    tournament::tournament,
};

pub mod ephemeral_state;
mod leaderboard;

// The table name needs to be plural since match is a rust keyword
/// # Match
/// Fullfills the role of providing configuration to the associated server and
/// executes the match on a Trackmania Server.
/// Also holds the Rules to reconstruct a Leaderboard for the match.
///
/// ## Lifecycle
/// Is represented and can be queried via the [MatchStatus]
/// and consists of:
/// - *Created.* In order to advance to the next stage a valid configuration for
/// pre_match and match fields need to be present. Advances to [MatchStatus::Configuring].
/// - *Configured.* Advances to [MatchStatus::Upcoming].
/// - *Captured Server.* Capturing describes the process of assigning a
/// Server from the pool to the Match. The server is locked till the match
/// releases it again. Advances to [MatchStatus::PreMatch]
/// - *Start.* Can be called manually, with a schedule or with rules.  
/// If the ephemeral state matches the desired state. Advances to [MatchStatus::Live].
/// - *End.* The match has concluded. Loads the post_match_config if it is present. Releases
/// the captured server. Advances to [MatchStatus::Ended].
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tm_match, public))]
pub struct TmMatch {
    #[auto_inc]
    #[primary_key]
    pub(crate) id: u64,

    /// The tournament this match is associated with.
    tournament_id: u64,
    parent_id: u64,

    scheduling: Scheduling,

    registration: Registration,

    /// The assigned server that will be used by this match.
    server_id: Option<String>,

    /// The moment the server is captured by the match the pre_match_config gets loaded in.
    /// Only if it is defined. Useful for hiding tournament maps till the actual start.
    pre_match_config: Option<ServerConfig>,
    /// If the match is started this config gets loaded.
    /// Has to be specified before your able to advance into Upcoming.
    match_config: Option<ServerConfig>,
    post_match_config: Option<ServerConfig>,

    status: MatchStatus,
    leaderboard: MatchLeaderboardRules,
    ephemeral_state: EphemeralState,
}

impl TmMatch {
    /// Evaluates is the Match is in the "Match" state of its lifecycle.
    pub fn is_live(&self) -> bool {
        self.status == MatchStatus::Live
    }

    pub fn get_tournament(&self) -> u64 {
        self.tournament_id
    }

    pub fn get_ephemeral_state(&self) -> EphemeralState {
        self.ephemeral_state
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
    /// Allows to change all associated configurations of the Match.
    Configuring,
    /// No changes to the configuration can be made anymore.
    Upcoming,
    PreMatch,
    Live,
    /// Match is immutable and achived.
    Ended,
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_match(
    ctx: &ReducerContext,
    tournament_id: u64,
    parent_id: u64,
    with_config: Option<u64>,
    auto_provisioning_server: bool,
) -> Result<(), String> {
    //TODO authorization
    ctx.auth()?;

    // Create an uncommitted match
    let tm_match = TmMatch {
        id: 0,
        parent_id,
        tournament_id,
        status: MatchStatus::Configuring,
        server_id: if auto_provisioning_server { None } else { None },
        pre_match_config: None,
        match_config: None,
        post_match_config: None,
        leaderboard: MatchLeaderboardRules::new(),
        ephemeral_state: EphemeralState::new(),
        scheduling: Scheduling::Manual,
        registration: Registration::Open,
    };

    if ctx.db.tournament().id().find(parent_id).is_none() {
        return Err("Invalid tournament".into());
    };

    let Some(mut parent_competition) = ctx.db.competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    let tm_match = ctx.db.tm_match().try_insert(tm_match)?;
    parent_competition.add_match(tm_match.id);
    ctx.db.competition().id().update(parent_competition);

    Ok(())
}

/// Assigns a server to the selected match.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_assign_server(ctx: &ReducerContext, to: u64, server_id: String) {
    //TODO authorization
    if let Some(mut server) = ctx.db.tm_server().id().find(&server_id)
        && server.active_match().is_none()
        && let Some(stage_match) = ctx.db.tm_match().id().find(to)
        && stage_match.status == MatchStatus::Configuring
    {
        let tm_match = ctx.db.tm_match().id().update(TmMatch {
            server_id: Some(server_id),
            ..stage_match
        });

        server.set_active_match(tm_match.id);

        ctx.db.tm_server().id().update(server);
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_configured(ctx: &ReducerContext, id: u64) {
    //TODO authorization
    if let Some(mut tm_match) = ctx.db.tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
    {
        tm_match.status = MatchStatus::Upcoming;
        ctx.db.tm_match().id().update(tm_match);
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
    if let Some(mut stage_match) = ctx.db.tm_match().id().find(id) {
        stage_match.match_config = Some(config);
        ctx.db.tm_match().id().update(stage_match);
    }
}

/// If the match is fully configured and ready start.
/// This can also serve as a manual override for scheduled matches.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn try_start(ctx: &ReducerContext, match_id: u64) {
    //TODO authorization
    if let Some(mut stage_match) = ctx.db.tm_match().id().find(match_id)
        && let Some(server) = &stage_match.server_id
        && let Some(mut server) = ctx.db.tm_server().id().find(server)
        && let Some(config) = &stage_match.match_config
        && stage_match.status == MatchStatus::Upcoming
    {
        server.set_config(config.clone());
        stage_match.status = MatchStatus::Live;
        ctx.db.tm_match().id().update(stage_match);
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
