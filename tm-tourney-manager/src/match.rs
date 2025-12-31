use spacetimedb::{Query, ReducerContext, SpacetimeType, Table, ViewContext, view};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::{
    auth::Authorization,
    competition::tab_competition,
    r#match::match_state::{TmMatchState, tab_tm_match_state},
    raw_server::tab_raw_server_online,
};

pub mod event;
pub mod match_state;
pub mod players;
pub mod template;

/// # Match
/// Fullfills the role of providing configuration to the associated server and
/// executes the match on a Trackmania Server.
/// Also holds the Rules to reconstruct a Leaderboard for the match.
///
/// ## Lifecycle
/// Is represented and can be queried via the [MatchStatus]
/// and consists of:
/// - *Created.* In order to advance to the next stage a valid configuration for
///  match_config need to be present. The same config will be used for pre_match if not overridden.
///  Advances to [MatchStatus::Configuring].
/// - *Configured.* Advances to [MatchStatus::Upcoming].
/// - *Captured Server.* Capturing describes the process of assigning a
/// Server from the pool to the Match. The server is locked till the match
/// releases it again. Advances to [MatchStatus::PreMatch]
/// - *Start.* Can be called manually, with a schedule or with rules.  
/// If the ephemeral state matches the desired state. Advances to [MatchStatus::Live].
/// - *End.* The match has concluded. Loads the post_match_config if it is present. Releases
/// the captured server. Advances to [MatchStatus::Ended].
#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tab_tm_match))]
pub struct TmMatchV1 {
    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    /// The tournament this match is associated with.
    tournament_id: u32,
    competition_id: u32,

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
}

impl TmMatchV1 {
    /// Evaluates is the Match is in the "Match" state of its lifecycle.
    pub fn is_live(&self) -> bool {
        self.status == MatchStatus::Live
    }

    pub fn get_tournament(&self) -> u32 {
        self.tournament_id
    }

    pub fn get_comp_id(&self) -> u32 {
        self.competition_id
    }

    pub(crate) fn end_match(&mut self) {
        self.status = MatchStatus::Ended;
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
    competition_id: u32,
    with_template: Option<u32>,
    //TODO: how to auto provision good?
    // maybe remove it from here and always auto assign from owned servers if not done manually in time.
    // THis would be done when switching to upcoming.
    //auto_provisioning_server: bool,
) -> Result<(), String> {
    ctx.get_user()?;

    let Some(parent_competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    // Create an uncommitted match
    let tm_match = TmMatchV1 {
        id: 0,
        competition_id,
        tournament_id: parent_competition.get_tournament(),
        status: MatchStatus::Configuring,
        server_id: None,
        pre_match_config: None,
        match_config: None,
        post_match_config: None,
    };

    let tm_match = ctx.db.tab_tm_match().try_insert(tm_match)?;

    ctx.db.tab_tm_match_state().try_insert(TmMatchState {
        id: tm_match.id,
        restarted: 0,
        round: 0,
        warmup: 0,
        is_warmup: false,
        paused: false,
    })?;

    Ok(())
}

/// Assigns a server to the selected match.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_assign_server(ctx: &ReducerContext, to: u32, server_id: String) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut server) = ctx.db.tab_raw_server_online().tm_login().find(&server_id)
        && server.active_match().is_none()
        && let Some(stage_match) = ctx.db.tab_tm_match().id().find(to)
        && stage_match.status == MatchStatus::Configuring
    {
        let tm_match = ctx.db.tab_tm_match().id().update(TmMatchV1 {
            server_id: Some(server_id),
            ..stage_match
        });

        server.set_active_match(tm_match.id);

        ctx.db.tab_raw_server_online().tm_login().update(server);
    }
    Ok(())
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
        && let Some(tm_server_id) = &tm_match.server_id
        && tm_match.match_config.is_some()
    {
        tm_match.status = MatchStatus::Upcoming;

        // Send the configuration of the corresponding match to the associated server.
        let Some(mut tm_server) = ctx.db.tab_raw_server_online().tm_login().find(tm_server_id)
        else {
            return Err(format!("No server with id {tm_server_id} could be found"));
        };
        if tm_match.pre_match_config.is_some() {
            tm_server.set_config(tm_match.pre_match_config.clone().unwrap());
        } else {
            tm_server.set_config(tm_match.match_config.clone().unwrap());
        }

        ctx.db.tab_tm_match().id().update(tm_match);

        ctx.db.tab_raw_server_online().tm_login().update(tm_server);
    }
    Ok(())
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn update_pre_match_config(
    ctx: &ReducerContext,
    id: u32,
    config: ServerConfig,
) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
    {
        tm_match.pre_match_config = Some(config);
        ctx.db.tab_tm_match().id().update(tm_match);
        Ok(())
    } else {
        Err(format!("Match with id: {id} not found."))
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn update_match_config(
    ctx: &ReducerContext,
    id: u32,
    config: ServerConfig,
) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
    {
        tm_match.match_config = Some(config);
        ctx.db.tab_tm_match().id().update(tm_match);
        Ok(())
    } else {
        Err(format!("Match with id: {id} not found."))
    }
}

/// If the match is fully configured and ready start.
/// This can also serve as a manual override for scheduled matches.
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn try_start_match(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(match_id)
        // Match needs an assigned server
        && let Some(server) = &tm_match.server_id
        //The assigned server needs to be valid
        && let Some(mut server) = ctx.db.tab_raw_server_online().tm_login().find(server)
        && let Some(config) = &tm_match.match_config
        && tm_match.status == MatchStatus::Upcoming
    {
        server.set_config(config.clone());
        tm_match.status = MatchStatus::Live;
        ctx.db.tab_tm_match().id().update(tm_match);
        ctx.db.tab_raw_server_online().tm_login().update(server);
    }
    Ok(())
}

#[view(name=tm_match,public)]
fn tm_match(ctx: &ViewContext) -> Query<TmMatchV1> {
    ctx.from.tab_tm_match().build()
}
