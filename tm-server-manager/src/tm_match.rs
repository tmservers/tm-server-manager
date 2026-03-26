use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};
use tm_server_types::config::ServerConfig;

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        node::{NodeHandle, NodeWrite},
        server_pool::TabCompetitionServerPoolRead,
        tab_competition,
    },
    raw_server::{
        TabRawServerWrite,
        config::{RawServerConfig, tab_raw_server_config},
        destination::TabRawServerDestinationWrite,
        occupation::{TabRawServerOccupationRead, TabRawServerOccupationWrite},
        tab_raw_server,
    },
    tm_match::{
        state::{MatchState, tab_match_state},
        template::match_template_instantiate,
    },
};

pub mod event;
pub mod leaderboard;
pub mod replay;
pub mod state;
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
#[table(accessor= tab_match)]
pub struct TmMatchV1 {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    #[index(hash)]
    parent_id: u32,

    /// The moment the server is captured by the match the pre_match_config gets loaded in.
    /// Only if it is defined. Useful for hiding project maps till the actual start.
    pre_config: u32,
    /// If the match is started this config gets loaded.
    /// Has to be specified before your able to advance into Upcoming.
    config: u32,

    status: MatchStatus,

    auto_provision_server: bool,
    //Whether the match is open for all to join or restricted.
    open: bool,
    template: bool,
}

impl TmMatchV1 {
    pub fn get_config_id(&self) -> u32 {
        match self.status {
            MatchStatus::Configuring => {
                panic!("should not ask for a config if match is configuring.")
            }
            MatchStatus::Configured => {
                panic!("should not ask for a config if match is configured.")
            }
            MatchStatus::Preparation => {
                if self.pre_config != 0 {
                    self.pre_config
                } else {
                    self.config
                }
            }
            MatchStatus::Live => self.config,
            MatchStatus::Ended => self.config,
            MatchStatus::Locked => {
                panic!("should not ask for a config if match is locked.")
            }
        }
    }

    /// Evaluates is the Match is in the "Match" state of its lifecycle.
    pub fn is_live(&self) -> bool {
        self.status == MatchStatus::Live
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn status(&self) -> MatchStatus {
        self.status
    }

    pub fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn instantiate(mut self, parent_id: u32, stay_template: bool) -> Self {
        self.template = stay_template;
        self.parent_id = parent_id;
        self.id = 0;
        self
    }

    pub(crate) fn end_match(&mut self) {
        self.status = MatchStatus::Ended;
    }
}

#[derive(Debug, PartialEq, Eq, SpacetimeType, Clone, Copy)]
pub enum MatchStatus {
    /// Allows to change all associated configurations of the Match.
    Configuring,
    Configured,
    /// No changes to the pre_match configuration can be made anymore.
    Preparation,
    /// No changes to the match configuration can be made anymore.
    Live,
    /// Match is immutable and achived.
    /// Loads the post match config if present.
    Ended,
    Locked,
}

impl MatchStatus {
    fn before_preparation(&self) -> bool {
        match self {
            MatchStatus::Configuring => true,
            MatchStatus::Configured => true,
            MatchStatus::Preparation => false,
            MatchStatus::Live => false,
            MatchStatus::Ended => false,
            MatchStatus::Locked => false,
        }
    }
}

#[reducer]
pub fn match_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    with_template: u32,
) -> Result<(), String> {
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CREATE)
        .authorize()?;

    if parent_competition.is_template() {
        return Err(
            "Cannot add a normal match to a template. Try do add a template match to id.".into(),
        );
    }

    // Try to load template if provided
    if with_template != 0 {
        match_template_instantiate(ctx, with_template)?;
    } else {
        // Create an uncommitted match
        let tm_match = TmMatchV1 {
            id: 0,
            parent_id,
            name,
            status: MatchStatus::Configuring,
            pre_config: 0,
            config: 0,
            auto_provision_server: true,
            template: false,
            open: false,
        };

        let tm_match = ctx.db.tab_match().try_insert(tm_match)?;
        ctx.node_create(NodeHandle::MatchV1(tm_match.id))?;
    }

    Ok(())
}

/// Assigns a server to the selected match.
/// This is only possible if the match is configuring or down
/// the server is not already occupied
/// the user has the permission to assign servers in the project
/// and the server is lended to the project.
#[reducer]
pub fn match_assign_server(ctx: &ReducerContext, to: u32, server_id: u32) -> Result<(), String> {
    let Some(tm_match) = ctx.db.tab_match().id().find(to) else {
        return Err("Supplied match was not found!".into());
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_ASSIGN_SERVER)
        .authorize()?;

    if tm_match.status != MatchStatus::Configuring && tm_match.status != MatchStatus::Configured {
        return Err(
            "Match is currently not getting configured so assigning a new server is impossible."
                .into(),
        );
    }

    if ctx.raw_server_is_occupied(server_id) {
        return Err("Server is already occupied! Cannot assign!".into());
    }

    if ctx.db.tab_raw_server().id().find(server_id).is_none() {
        return Err("Server with id was not found!".into());
    };

    if ctx
        .server_pool_available(tm_match.parent_id)
        .into_iter()
        .any(|s| s.id == server_id)
    {
        return Err("Server is not lended to the project".into());
    }

    ctx.raw_server_occupation_add(NodeHandle::MatchV1(to), server_id)?;

    Ok(())
}

#[reducer]
pub fn match_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut tm_match) = ctx.db.tab_match().id().find(id) else {
        return Err("Match was mot found!".into());
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    if tm_match.status != MatchStatus::Configuring {
        return Err("Match is not in configuring state".into());
    }
    tm_match.status = MatchStatus::Configured;

    ctx.db.tab_match().id().update(tm_match);

    Ok(())
}

#[reducer]
pub fn match_update_pre_config(
    ctx: &ReducerContext,
    id: u32,
    config_id: u32,
) -> Result<(), String> {
    if let Some(mut tm_match) = ctx.db.tab_match().id().find(id)
        && tm_match.status == MatchStatus::Configuring
    {
        ctx.auth_builder(tm_match.parent_id)
            .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
            .authorize()?;
        tm_match.pre_config = config_id;
        ctx.db.tab_match().id().update(tm_match);
        Ok(())
    } else {
        Err(format!("Match {id} not found or in wrong state."))
    }
}

#[reducer]
pub fn match_update_config(
    ctx: &ReducerContext,
    id: u32,
    config: ServerConfig,
) -> Result<(), String> {
    let Some(mut tm_match) = ctx.db.tab_match().id().find(id) else {
        return Err("Match was mot found!".into());
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    if !tm_match.status.before_preparation() {
        return Err("Too late to set configuration".into());
    }

    //TODO cleanup old/orphaned configs. Should i do this with a mapping table or just always instantiate the config or keep track of this in the match?
    //TODO also check if it is empty (0) or if smth was there before.
    let cfg = ctx
        .db
        .tab_raw_server_config()
        .try_insert(RawServerConfig::new(config))?;
    tm_match.config = cfg.id;
    ctx.db.tab_match().id().update(tm_match);
    Ok(())
}

/// If the match is fully configured and ready start.
/// This can also serve as a manual override for scheduled matches.
#[reducer]
fn match_set_preparation(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    let Some(tm_match) = ctx.db.tab_match().id().find(match_id) else {
        return Err("Match not found!".into());
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    authorized_match_set_preparation(ctx, match_id)
}

pub fn authorized_match_set_preparation(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    let Some(mut tm_match) = ctx.db.tab_match().id().find(match_id) else {
        return Err("Match not found!".into());
    };

    if tm_match.is_template() {
        return Err("Method cannot be called on templates.".into());
    }

    if tm_match.status == MatchStatus::Configuring {
        return Err("Match is still getting configured.".into());
    }
    if tm_match.config == 0 {
        return Err(
            "Match needs a configuration in order to advance to the upcoming state.".into(),
        );
    }

    let competition_id = tm_match.parent_id;

    if ctx
        .occupation_with_occupier(NodeHandle::MatchV1(match_id))
        .is_some()
    {
        tm_match.status = MatchStatus::Preparation;
        ctx.db.tab_match().id().update(tm_match);

        ctx.db
            .tab_match_state()
            .try_insert(MatchState::new(match_id))?;
    } else if tm_match.auto_provision_server {
        ctx.raw_server_pool_assign(NodeHandle::MatchV1(match_id))?;

        tm_match.status = MatchStatus::Preparation;
        ctx.db.tab_match().id().update(tm_match);

        ctx.db
            .tab_match_state()
            .try_insert(MatchState::new(match_id))?;
    } else {
        return Err("Match has auto provisioning turned off and no server assigned! Cannot start the match!".into());
    };

    ctx.destination_claim(NodeHandle::MatchV1(match_id))?;

    Ok(())
}

/// If the match is fully configured and ready start.
/// This can also serve as a manual override for scheduled matches.
#[reducer]
pub fn match_try_start(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    let Some(mut tm_match) = ctx.db.tab_match().id().find(match_id) else {
        return Err("Match not found!".into());
    };

    if tm_match.is_template() {
        return Err("Method cannot be called on templates.".into());
    }

    if tm_match.status != MatchStatus::Preparation {
        return Err("Match needs to be prepared in order to be started.".into());
    }

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    if ctx
        .occupation_with_occupier(NodeHandle::MatchV1(match_id))
        .is_none()
    {
        return Err("No server is assigned to the match.".into());
    };

    //TODO this is depending on player state (e.g. is there need to be specific players present are all there?)
    tm_match.status = MatchStatus::Live;
    ctx.db.tab_match().id().update(tm_match);

    ctx.db
        .tab_match_state()
        .try_insert(MatchState::new(match_id))?;

    Ok(())
}

#[reducer]
pub fn match_delete(ctx: &ReducerContext, match_id: u32) -> Result<(), String> {
    let Some(tm_match) = ctx.db.tab_match().id().find(match_id) else {
        return Err(format!("Match with id: {match_id} not found."));
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_DELETE)
        .authorize()?;

    if !ctx.db.tab_match().id().delete(match_id) {
        return Err(format!("Match with id: {match_id} not found."));
    }

    let handle = NodeHandle::MatchV1(match_id);

    ctx.node_delete(handle)?;

    Ok(())
}

/* #[view(accessor=tm_match,public)]
fn tm_match(ctx: &ViewContext) -> impl Query<TmMatchV1> {
    ctx.from.tab_match()
} */

/* pub(crate) trait MatchRead {
}
impl<Db: spacetimedb::DbContext> MatchRead for Db {

}

pub(crate) trait MatchWrite: MatchRead {}
impl<Db: spacetimedb::DbContext<DbView = spacetimedb::Local>> MatchWrite for Db {} */
