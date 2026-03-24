use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        node::{NodeHandle, TabCompetitionNodePosition, tab_competition_node_position},
        tab_competition,
    },
    raw_server::{
        occupation::{TabRawServerOccupationRead, TabRawServerOccupationWrite},
        tab_raw_server,
    },
    tm_server::template::server_template_instantiate,
};

pub mod template;

#[table(accessor= tab_server)]
pub struct TmServerV1 {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    #[index(hash)]
    parent_id: u32,

    config: u32,

    status: ServerStatus,

    open: bool,
}

impl TmServerV1 {
    pub(crate) fn get_config_id(&self) -> u32 {
        self.config
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }
}

#[derive(Debug, PartialEq, Eq, SpacetimeType, Clone, Copy)]
pub enum ServerStatus {
    Configuring,
    Ongoing,
}

#[reducer]
pub fn server_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    with_template: u32,
) -> Result<(), String> {
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(parent_id)
        //TODO
        //.permission(CompetitionPermissionsV1::SERVER_CREATE)
        .authorize()?;

    if parent_competition.is_template() {
        return Err(
            "Cannot add a normal server to a template. Try do add a template server to id.".into(),
        );
    }

    // Try to load template if provided
    if with_template != 0 {
        server_template_instantiate(ctx, with_template)?;
    } else {
        // Create an uncommitted match
        let tm_server = TmServerV1 {
            name,
            id: 0,
            parent_id,
            config: 0,
            status: ServerStatus::Configuring,
            open: true,
        };

        let tm_match = ctx.db.tab_server().try_insert(tm_server)?;

        ctx.db
            .tab_competition_node_position()
            .try_insert(TabCompetitionNodePosition::new(
                NodeHandle::MatchV1(tm_match.id),
                tm_match.parent_id,
            ))?;
    }

    Ok(())
}

#[reducer]
pub fn server_assign_raw_server(
    ctx: &ReducerContext,
    to: u32,
    server_id: u32,
) -> Result<(), String> {
    let Some(tm_match) = ctx.db.tab_server().id().find(to) else {
        return Err("Supplied match was not found!".into());
    };

    ctx.auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_ASSIGN_SERVER)
        .authorize()?;

    if ctx.raw_server_is_occupied(server_id) {
        return Err("Server is already occupied! Cannot assign!".into());
    }

    if ctx.db.tab_raw_server().id().find(server_id).is_none() {
        return Err("Server with id was not found!".into());
    };

    //TODO recurse upwards through the competition tree.
    /* if competition_available_server_pool(&ctx.as_read_only())
        .into_iter()
        .any(|s| s.id == server_id)
    {
        return Err("Server is not lended to the project".into());
    } */

    ctx.raw_server_occupation_add(NodeHandle::ServerV1(to), server_id)?;

    Ok(())
}

#[reducer]
pub fn server_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut tm_server) = ctx.db.tab_server().id().find(id) else {
        return Err("Server was mot found!".into());
    };

    ctx.auth_builder(tm_server.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    if tm_server.status != ServerStatus::Configuring {
        return Err("Match is not in configuring state".into());
    }
    tm_server.status = ServerStatus::Ongoing;

    ctx.db.tab_server().id().update(tm_server);

    Ok(())
}
