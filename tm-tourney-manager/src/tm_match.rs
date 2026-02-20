use spacetimedb::{Query, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};
use tm_server_types::{config::ServerConfig, event::Event};

use crate::{
    authorization::Authorization,
    competition::{
        connection::{
            NodeKindHandle,
            node_position::{TabCompetitionNodePosition, tab_competition_node_position},
            tab_competition_connection,
        },
        tab_competition,
    },
    raw_server::{
        RawServerOccupation, available_server_pool, raw_server_pool, tab_raw_server,
        tab_raw_server_occupation,
    },
    tm_match::{
        match_state::{TmMatchState, tab_tm_match_state},
        template::tab_match_template,
    },
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
#[table(name = tab_tm_match)]
pub struct TmMatchV1 {
    /* /// The assigned server that is currently used by this match.
    server_id: Option<String>, */
    name: String,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    /// The tournament this match is associated with.
    tournament_id: u32,
    competition_id: u32,

    /// The moment the server is captured by the match the pre_match_config gets loaded in.
    /// Only if it is defined. Useful for hiding tournament maps till the actual start.
    pre_match_config: u32,
    /// If the match is started this config gets loaded.
    /// Has to be specified before your able to advance into Upcoming.
    match_config: u32,
    post_match_config: u32,

    status: MatchStatus,

    auto_provision_server: bool,
}

impl TmMatchV1 {
    pub fn get_config_id(&self) -> u32 {
        match self.status {
            MatchStatus::Configuring => 0,
            MatchStatus::Upcoming => self.pre_match_config,
            MatchStatus::Live => self.match_config,
            MatchStatus::Ended => self.post_match_config,
        }
    }

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

#[derive(Debug, PartialEq, Eq, SpacetimeType)]
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

#[reducer]
pub fn match_create(
    ctx: &ReducerContext,
    name: String,
    competition_id: u32,
    with_template: Option<u32>,
    //TODO: how to auto provision good?
    // maybe remove it from here and always auto assign from owned servers if not done manually in time.
    // THis would be done when switching to upcoming.
    //auto_provisioning_server: bool,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    /* ctx.tournament_permissions(tournament_id, &user)?
    .permission(TournamentPermissionsV1::TOURNAMENT_EDIT_NAME)
    .check()?; */

    let Some(parent_competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    // Create an uncommitted match
    let mut tm_match = TmMatchV1 {
        id: 0,
        competition_id,
        tournament_id: parent_competition.get_tournament(),
        name,
        status: MatchStatus::Configuring,
        //server_id: None,
        pre_match_config: 0,
        match_config: 0,
        post_match_config: 0,
        auto_provision_server: true,
    };

    // Try to load template if provided
    if let Some(template) = with_template {
        let Some(template) = ctx.db.tab_match_template().id().find(template) else {
            return Err("Template not found.".into());
        };
        tm_match.match_config = template.get_config_id()
    }

    let tm_match = ctx.db.tab_tm_match().try_insert(tm_match)?;

    ctx.db
        .tab_competition_node_position()
        .try_insert(TabCompetitionNodePosition::new(
            NodeKindHandle::MatchV1(tm_match.id),
            tm_match.competition_id,
        ))?;

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
#[reducer]
pub fn match_assign_server(ctx: &ReducerContext, to: u32, server_id: u32) -> Result<(), String> {
    //TODO permissions
    let user = ctx.get_user()?;
    if let Some(server) = ctx.db.tab_raw_server().id().find(server_id)
        && server.account_id == user.account_id
        && server.is_verified()
        && let Some(tm_match) = ctx.db.tab_tm_match().id().find(to)
        && tm_match.status == MatchStatus::Configuring
    {
        /* if let Some(mut occupation) = ctx
            .db
            .tab_raw_server_occupation()
            .server_id()
            .find(server.id)
        {} */
        //TODO check permissions
        ctx.db
            .tab_raw_server_occupation()
            .server_id()
            .try_insert_or_update(RawServerOccupation::new(to, server_id))?;
    }
    Ok(())
}

#[reducer]
pub fn match_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
        && tm_match.match_config != 0
    {
        tm_match.status = MatchStatus::Upcoming;

        // Send the configuration of the corresponding match to the associated server.
        /* let Some(mut tm_server) = ctx
            .db
            .tab_raw_server_occupation()
            .match_id()
            .find(tm_server_id)
        else {
            return Err(format!("No server with id {tm_server_id} could be found"));
        };
        if tm_match.pre_match_config.is_some() {
            //tm_server.set_config(tm_match.pre_match_config.clone().unwrap());
        } else {
            //tm_server.set_config(tm_match.match_config.clone().unwrap());
        } */

        /* ctx.db.tab_tm_match().id().update(tm_match);

        ctx.db.tab_raw_server().server_login().update(tm_server); */
    }
    Ok(())
}

#[reducer]
pub fn match_update_pre_config(
    ctx: &ReducerContext,
    id: u32,
    config_id: u32,
) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
    {
        tm_match.pre_match_config = config_id;
        ctx.db.tab_tm_match().id().update(tm_match);
        Ok(())
    } else {
        Err(format!("Match with id: {id} not found."))
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn match_update_config(
    ctx: &ReducerContext,
    id: u32,
    config: ServerConfig,
) -> Result<(), String> {
    ctx.get_user()?;
    if let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
    {
        //TODO
        //tm_match.match_config = Some(config);
        ctx.db.tab_tm_match().id().update(tm_match);
        Ok(())
    } else {
        Err(format!("Match with id: {id} not found."))
    }
}

/// If the match is fully configured and ready start.
/// This can also serve as a manual override for scheduled matches.
#[reducer]
pub fn match_try_start(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(match_id) else {
        return Err("Match not found!".into());
    };

    //TODO verify more things
    if let Some(server) = ctx
        .db
        .tab_raw_server_occupation()
        .match_id()
        .filter(tm_match.id)
        .next()
    {
        tm_match.status = MatchStatus::Live;
        ctx.db.tab_tm_match().id().update(tm_match);

        return Ok(());
    }

    if tm_match.auto_provision_server {
        let available_servers = available_server_pool(&ctx.as_read_only());
        if available_servers.is_empty() {
            return Err("No server is assigned to the match and there are no servers left to auto provision. Cannot start the match!".into());
        }

        ctx.db
            .tab_raw_server_occupation()
            .try_insert(RawServerOccupation::new(match_id, available_servers[0].id))?;

        Ok(())
    } else {
        Err("Match has auto provisioning turned off and no server assigned! Cannot start the match!".into())
    }
}

// Delete a match
#[reducer]
pub fn match_delete(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    ctx.get_user()?;

    let Some(tm_match) = ctx.db.tab_tm_match().id().find(match_id) else {
        return Err(format!("Match with id: {match_id} not found."));
    };
    if !ctx.db.tab_tm_match().id().delete(match_id) {
        return Err(format!("Match with id: {match_id} not found."));
    }

    let node_ref = NodeKindHandle::MatchV1(match_id);

    // This should only ever delete one but we dont have muulti col unique index for now
    for node in ctx
        .db
        .tab_competition_node_position()
        .node_position()
        .filter(node_ref.split())
    {
        ctx.db.tab_competition_node_position().id().delete(node.id);
    }

    for node in ctx
        .db
        .tab_competition_connection()
        .competition_id()
        .filter(tm_match.competition_id)
    {
        if node.node_from() == node_ref || node.node_to() == node_ref {
            ctx.db.tab_competition_connection().delete(node);
        }
    }

    Ok(())
}

#[view(name=tm_match,public)]
fn tm_match(ctx: &ViewContext) -> Query<TmMatchV1> {
    ctx.from.tab_tm_match().build()
}
