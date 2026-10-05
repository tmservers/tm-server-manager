use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};
use tm_server_types::config::ServerConfigV2;

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1, CompetitionRead,
        node::{NodeHandle, NodeWrite, Vec2},
        server_pool::TabCompetitionServerPoolRead,
    },
    raw_server::{
        TabRawServerRead,
        config::{RawServerContigRead, RawServerContigWrite},
        occupation::{TabRawServerOccupationRead, TabRawServerOccupationWrite},
    },
};

#[table(accessor= tab_server,vis_private)]
pub struct ServerV1 {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    #[index(hash)]
    parent_id: u32,

    #[index(hash)]
    config: u32,

    status: ServerStatus,

    open: bool,
    template: bool,
    auto_provision: bool,
}

impl ServerV1 {
    pub(crate) fn get_config_id(&self) -> u32 {
        self.config
    }

    pub(crate) fn parent_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

     pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug, PartialEq, Eq, SpacetimeType, Clone, Copy)]
pub enum ServerStatus {
    Configuring,
    Ongoing,
}

#[reducer]
fn server_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    position: Vec2,
    with_template: u32,
    as_template: bool,
) -> Result<(), String> {
    let parent_competition = ctx.competition_find(parent_id)?;

    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::SERVER_CREATE)
        .authorize()?;

    if parent_competition.is_template() {
        return Err(
            "Cannot add a normal server to a template. Try do add a template server to id.".into(),
        );
    }

    if with_template != 0 {
        todo!()
    } else {
        ServerWrite::server_create(ctx, name, parent_id, position, None, as_template)?;
    }

    Ok(())
}

#[reducer]
fn server_remove_raw_server(ctx: &ReducerContext, server_id: u32) -> Result<(), String> {
    let Some(mut tm_server) = ctx.db.tab_server().id().find(server_id) else {
        return Err("Supplied match was not found!".into());
    };

    if tm_server.is_template() {
        return Err("Cannot do that for a template".into());
    }

    ctx.auth_builder(tm_server.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_ASSIGN_SERVER)
        .authorize()?;

    if ctx.raw_server_is_occupied(server_id) {
        ctx.raw_server_occupation_remove(NodeHandle::ServerV1(server_id))?;
    } else {
        return Err("Server was not occupied!".into());
    }

    tm_server.status = ServerStatus::Configuring;

    ctx.db.tab_server().id().update(tm_server);

    Ok(())
}

#[reducer]
fn server_assign_raw_server(
    ctx: &ReducerContext,
    server_id: u32,
    raw_server_id: u32,
) -> Result<(), String> {
    let Some(tm_server) = ctx.db.tab_server().id().find(server_id) else {
        return Err("Supplied match was not found!".into());
    };

    if tm_server.is_template() {
        return Err("Cannot do that for a template".into());
    }

    ctx.auth_builder(tm_server.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_ASSIGN_SERVER)
        .authorize()?;

    if ctx.raw_server_is_occupied(raw_server_id) {
        return Err("Server is already occupied! Cannot assign!".into());
    }

    // Check if server actually exists.
    ctx.raw_server_find(raw_server_id)?;

    if ctx
        .occupation_with_occupier(NodeHandle::ServerV1(server_id))
        .is_some()
    {
        ctx.raw_server_occupation_remove(NodeHandle::ServerV1(server_id))?;
    }

    if !ctx
        .server_pool_available(tm_server.parent_id)
        .into_iter()
        .any(|s| s.id == raw_server_id)
    {
        return Err("Server is not lended to the project".into());
    }

    ctx.raw_server_occupation_add(NodeHandle::ServerV1(server_id), raw_server_id)?;

    Ok(())
}

#[reducer]
fn server_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut tm_server) = ctx.db.tab_server().id().find(id) else {
        return Err("Server was mot found!".into());
    };

    if tm_server.is_template() {
        return Err("Cannot do that for a template".into());
    }

    ctx.auth_builder(tm_server.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    if tm_server.status != ServerStatus::Configuring {
        return Err("Match is not in configuring state".into());
    }

    if tm_server.config == 0 {
        return Err("Need a configuration in order to set server to configured.".into());
    }

    tm_server.status = ServerStatus::Ongoing;

    ctx.db.tab_server().id().update(tm_server);

    let raw_server = ctx
        .occupation_with_occupier(NodeHandle::ServerV1(id))
        .unwrap();
    ctx.emit_raw_server_config(raw_server, true)?;

    Ok(())
}

#[reducer]
fn server_configuring(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut tm_server) = ctx.db.tab_server().id().find(id) else {
        return Err("Server was mot found!".into());
    };

    if tm_server.is_template() {
        return Err("Cannot do that for a template".into());
    }

    ctx.auth_builder(tm_server.parent_id)
        //TODO
        //.permission(CompetitionPermissionsV1::MATCH_CONFIGURE)
        .authorize()?;

    if tm_server.status != ServerStatus::Ongoing {
        return Err("Match is not in configuring state".into());
    }

    tm_server.status = ServerStatus::Configuring;

    ctx.db.tab_server().id().update(tm_server);

    Ok(())
}

#[reducer]
fn server_config_override(
    ctx: &ReducerContext,
    to: u32,
    config: ServerConfigV2,
) -> Result<(), String> {
    let Some(mut tm_server) = ctx.db.tab_server().id().find(to) else {
        return Err("Supplied match was not found!".into());
    };

    ctx.auth_builder(tm_server.parent_id)
        //TODO
        //.permission(CompetitionPermissionsV1::SERVER_ASSIGN)
        .authorize()?;

    let configs = ctx.raw_server_config_references(tm_server.config);
    if configs.len() == 1 {
        ctx.raw_server_config_update(tm_server.config, config)?;
    } else {
        let config = ctx.raw_server_config_create(config, 0)?;
        tm_server.config = config.id;

        tm_server = ctx.db.tab_server().id().update(tm_server);
    }

    if tm_server.status == ServerStatus::Ongoing {
        let raw_server = ctx
            .occupation_with_occupier(NodeHandle::ServerV1(to))
            .unwrap();
        ctx.emit_raw_server_config(raw_server, true)?;
    }

    Ok(())
}

pub(crate) trait ServerRead {
    fn servers_with_config(&self, config_id: u32) -> impl Iterator<Item = ServerV1>;
    fn server_find(&self, server_id: u32) -> Result<ServerV1, String>;
    fn servers_with_competition(&self, competition_id: u32) -> impl Iterator<Item = ServerV1>;
}

impl<Db: spacetimedb::CtxDbRead> ServerRead for Db {
    fn servers_with_config(&self, config_id: u32) -> impl Iterator<Item = ServerV1> {
        self.db_read_only().tab_server().config().filter(config_id)
    }

    fn server_find(&self, server_id: u32) -> Result<ServerV1, String> {
        let Some(server) = self.db_read_only().tab_server().id().find(server_id) else {
            return Err("Server not found".into());
        };
        Ok(server)
    }

    fn servers_with_competition(&self, competition_id: u32) -> impl Iterator<Item = ServerV1> {
        self.db_read_only()
            .tab_server()
            .parent_id()
            .filter(competition_id)
    }
}

pub(crate) trait ServerWrite {
    fn server_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<ServerV1>,
        as_template: bool,
    ) -> Result<ServerV1, String>;
    fn server_name_edit(&self, match_id: u32, name: String) -> Result<(), String>;
}

impl<Db: spacetimedb::CtxDbWrite> ServerWrite for Db {
    fn server_name_edit(&self, match_id: u32, name: String) -> Result<(), String> {
        let Some(mut tm_match) = self.db().tab_server().id().find(match_id) else {
            return Err("Match not found.".into());
        };
        tm_match.name = name;
        self.db().tab_server().id().update(tm_match);

        Ok(())
    }

    fn server_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<ServerV1>,
        as_template: bool,
    ) -> Result<ServerV1, String> {
        let server = if let Some(mut template) = template {
            template.template = as_template; //TODO: Audit codepath for legal again.
            template.parent_id = parent_id;
            template.id = 0;
            template
        } else {
            ServerV1 {
                name,
                id: 0,
                parent_id,
                config: 0,
                status: ServerStatus::Configuring,
                open: true,
                template: false,
                auto_provision: true,
            }
        };

        let tm_match = self.db().tab_server().try_insert(server)?;
        self.node_create(NodeHandle::ServerV1(tm_match.id), position)?;

        Ok(tm_match)
    }
}

// Server and match config problem space mapping:
// How to clean up old configs which are orphaned or override them?
// Big problem: Are there crossovers between servers and matches?
//  -> This invariant should be impossible??? -> otherwise how to handle
//  -> Prefferably handle that case.
//

// -> Differentiate between update and override.
// -> Annotate a hash index for match and server configs.
// -> Always search all three things: server config. match config. match pre_config.
// -> when no other stuff is found then we can delete it or even update?
// -> override is conceptually obvious i guess -> we can update the old one even if override if its the only one.

// -> what should the confirmation be for updating a random config?
// -> maybe expose a procedure which warns you with the count of how many configs you are updating?

// Problem: what if there are matches in progress for a config update?
// -> does the update just fail?
// -> or does the update go through and override for live and finished matches?
