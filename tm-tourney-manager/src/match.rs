use spacetimedb::{ReducerContext, Table};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::{
    auth::Authorization,
    competition::competition,
    r#match::{leaderboard::MatchLeaderboardRules, match_state::MatchState},
    registration::RegistrationRules,
    scheduling::Scheduling,
    server::tm_server,
    tournament::tournament,
};

mod leaderboard;
pub mod match_state;

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
///  match_config need to be present. Tthe same config will be used for pre_match if not overridden.
///  Advances to [MatchStatus::Configuring].
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
    pub(crate) id: u32,

    /// The tournament this match is associated with.
    tournament_id: u32,
    competition_id: u32,

    scheduling: Scheduling,

    // qualified_entities: RegistrationRules,
    /// The assigned server that is currently used by this match.
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
    state: MatchState,
}

impl TmMatch {
    /// Evaluates is the Match is in the "Match" state of its lifecycle.
    pub fn is_live(&self) -> bool {
        self.status == MatchStatus::Live
    }

    pub fn get_tournament(&self) -> u32 {
        self.tournament_id
    }

    pub fn get_match_state(&self) -> MatchState {
        self.state
    }

    pub(crate) fn end_match(&mut self) {
        self.status = MatchStatus::Ended;
    }

    pub fn add_server_event(&mut self, event: &Event) -> bool {
        // Not worth defining as an invariant for calling so need to be sure.
        if !self.is_live() {
            return false;
        }

        match event {
            Event::WarmupStart => self.state.enable_wu(),
            Event::WarmupEnd => self.state.disable_wu(),
            Event::WarmupStartRound(_) => self.state.new_wu_round(),
            Event::StartRoundStart(_) => self.state.new_round(),
            _ => return false,
        }
        log::warn!("{:#?}", self.state);
        true
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum MatchStatus {
    /// Allows to change all associated configurations of the Match.
    Configuring,
    /// No changes to the pre_match configuration can be made anymore.
    Upcoming,
    /// No changes to the match configuration can be made anymore.
    Live,
    /// Match is immutable and achived.
    /// Loads the post match config if present.
    Ended,
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn create_match(
    ctx: &ReducerContext,
    tournament_id: u32,
    competition_id: u32,
    with_template: Option<u32>,
    auto_provisioning_server: bool,
) -> Result<(), String> {
    ctx.auth_user()?;

    // Create an uncommitted match
    let tm_match = TmMatch {
        id: 0,
        competition_id,
        tournament_id,
        status: MatchStatus::Configuring,
        server_id: if auto_provisioning_server { None } else { None },
        pre_match_config: None,
        match_config: None,
        post_match_config: None,
        leaderboard: MatchLeaderboardRules::new(),
        state: MatchState::new(),
        scheduling: Scheduling::Manual,
        //registration_rules: RegistrationRules::Open,
    };

    if ctx.db.tournament().id().find(competition_id).is_none() {
        return Err("Invalid tournament".into());
    };

    let Some(mut parent_competition) = ctx.db.competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    let tm_match = ctx.db.tm_match().try_insert(tm_match)?;
    parent_competition.add_match(tm_match.id);
    ctx.db.competition().id().update(parent_competition);

    Ok(())
}

/// Assigns a server to the selected match.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_assign_server(ctx: &ReducerContext, to: u32, server_id: String) -> Result<(), String> {
    ctx.auth_user()?;
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
    Ok(())
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    ctx.auth_user()?;
    if let Some(mut tm_match) = ctx.db.tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
        && tm_match.server_id.is_some()
        && tm_match.match_config.is_some()
    {
        tm_match.status = MatchStatus::Upcoming;
        ctx.db.tm_match().id().update(tm_match);
    }
    Ok(())
}

/* #[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn update_pre_match_config(ctx: &ReducerContext, id: u32, config: ServerConfig) {
    //TODO authorization
    if let Some(mut stage_match) = ctx.db.stage_match().id().find(id) {
        stage_match.match_config = Some(config);
        ctx.db.stage_match().id().update(stage_match);
    }
} */

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn update_match_config(
    ctx: &ReducerContext,
    id: u32,
    config: ServerConfig,
) -> Result<(), String> {
    ctx.auth_user()?;
    if let Some(mut stage_match) = ctx.db.tm_match().id().find(id) {
        stage_match.match_config = Some(config);
        ctx.db.tm_match().id().update(stage_match);
        Ok(())
    } else {
        Err(format!("Match with id: {id} not found."))
    }
}

/// If the match is fully configured and ready start.
/// This can also serve as a manual override for scheduled matches.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn try_start(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    ctx.auth_user()?;
    if let Some(mut tm_match) = ctx.db.tm_match().id().find(match_id)
        // Match needs an assigned server
        && let Some(server) = &tm_match.server_id
        //The assigned server needs to be valid
        && let Some(mut server) = ctx.db.tm_server().id().find(server)
        && let Some(config) = &tm_match.match_config
        && tm_match.status == MatchStatus::Upcoming
    {
        server.set_config(config.clone());
        tm_match.status = MatchStatus::Live;
        ctx.db.tm_match().id().update(tm_match);
        ctx.db.tm_server().id().update(server);
    }
    Ok(())
}

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = match_template,public))]
pub struct MatchTemplate {
    #[auto_inc]
    #[primary_key]
    id: u32,

    creator: String,
}
