use spacetimedb::{CtxDbRead, ProcedureContext, ReducerContext, Table, procedure, reducer, table};
use tm_server_types::config::{ServerConfig, ServerConfigV2};

use crate::{
    authorization::Authorization, auto_inc_manual::AutoIncWrite, competition::node::NodeHandle,
    raw_server::occupation::TabRawServerOccupationRead, tm_match::MatchRead, tm_server::ServerRead,
};

#[table(accessor=tab_raw_server_config)]
struct RawServerConfig {
    #[auto_inc]
    #[primary_key]
    id: u32,

    config: ServerConfig,

    // This is a shared config associated with a competition.
    #[index(hash)]
    #[default(0)]
    competition_id: u32,
}

#[table(accessor=tab_raw_server_config_v2,vis_private)]
pub struct RawServerConfigV2 {
    #[primary_key]
    pub id: u32,

    // This is a shared config associated with a competition.
    #[index(hash)]
    #[default(0)]
    competition_id: u32,

    config: ServerConfigV2,
}

impl RawServerConfigV2 {
    pub fn new(config: ServerConfigV2, competition_id: u32) -> Self {
        Self {
            id: 0,
            config,
            competition_id,
        }
    }

    pub(crate) fn instantiate(mut self, parent_id: u32, ctx: &ReducerContext) -> Self {
        self.competition_id = parent_id;
        self.id = ctx.auto_inc::<tab_raw_server_config_v2__TableHandle>();
        self
    }
}

#[table(accessor=event_raw_server_state,event,public)]
struct EventRawServerState {
    #[primary_key]
    server_id: u32,
    config: ServerConfigV2,
    open: bool,
    occupied: bool,
    seamless: bool,
}

/* #[table(accessor=event_raw_server_state_v2,event,public)]
struct EventRawServerStateV2 {
    #[primary_key]
    server_id: u32,
    open: bool,
    occupied: bool,
    seamless: bool,
    config: ServerConfigV2,
} */

pub(crate) trait RawServerContigRead {
    fn raw_server_config_references(&self, config_id: u32) -> Vec<NodeHandle>;
    fn raw_server_config(&self, config_id: u32) -> Result<ServerConfigV2, String>;

    fn raw_server_config_exists(&self, config_id: u32) -> bool;

    fn raw_server_config_shared_competition(
        &self,
        competition_id: u32,
    ) -> impl Iterator<Item = RawServerConfigV2>;
}
impl<Db: spacetimedb::CtxDbRead> RawServerContigRead for Db {
    fn raw_server_config_references(&self, config_id: u32) -> Vec<NodeHandle> {
        let mut config_references = Vec::new();
        config_references.extend(
            self.matches_with_config(config_id)
                .map(|m| NodeHandle::MatchV1(m.id)),
        );
        config_references.extend(
            self.matches_with_pre_config(config_id)
                .map(|m| NodeHandle::MatchV1(m.id)),
        );
        config_references.extend(
            self.servers_with_config(config_id)
                .map(|m| NodeHandle::ServerV1(m.id)),
        );

        config_references
    }

    fn raw_server_config(&self, config_id: u32) -> Result<ServerConfigV2, String> {
        let Some(config) = self
            .db_read_only()
            .tab_raw_server_config_v2()
            .id()
            .find(config_id)
        else {
            return Err("Config with id could not be found".into());
        };

        Ok(config.config)
    }

    fn raw_server_config_exists(&self, config_id: u32) -> bool {
        self.db_read_only()
            .tab_raw_server_config_v2()
            .id()
            .find(config_id)
            .is_some()
    }

    fn raw_server_config_shared_competition(
        &self,
        competition_id: u32,
    ) -> impl Iterator<Item = RawServerConfigV2> {
        self.db_read_only()
            .tab_raw_server_config_v2()
            .competition_id()
            .filter(competition_id)
    }
}

pub(crate) trait RawServerContigWrite {
    fn raw_server_config_update(
        &self,
        config_id: u32,
        new_config: ServerConfigV2,
    ) -> Result<(), String>;

    fn raw_server_config_new(
        &self,
        new_config: ServerConfigV2,
        competition_id: u32,
    ) -> Result<u32, String>;

    fn emit_raw_server_config(&self, server_id: u32, seamless: bool) -> Result<(), String>;
}

impl<Db: spacetimedb::CtxDbWrite> RawServerContigWrite for Db {
    fn raw_server_config_update(
        &self,
        config_id: u32,
        new_config: ServerConfigV2,
    ) -> Result<(), String> {
        let Some(mut config) = self.db().tab_raw_server_config_v2().id().find(config_id) else {
            return Err("Config not found".into());
        };
        config.config = new_config;

        self.db().tab_raw_server_config_v2().id().update(config);

        Ok(())
    }

    /// The compeition_id determines if it is a shared config.
    /// If it is null it is a solo config if not then its associated with the compeition.
    fn raw_server_config_new(
        &self,
        new_config: ServerConfigV2,
        competition_id: u32,
    ) -> Result<u32, String> {
        let id = self
            .db()
            .tab_raw_server_config_v2()
            .try_insert(RawServerConfigV2 {
                id: self.auto_inc::<tab_raw_server_config_v2__TableHandle>(),
                competition_id,
                config: new_config,
            })?;

        Ok(id.id)
    }

    fn emit_raw_server_config(&self, server_id: u32, seamless: bool) -> Result<(), String> {
        let Some(node) = self.raw_server_occupation(server_id) else {
            return Err("Ocuupation not found.".into());
        };

        match node {
            crate::competition::node::NodeHandle::MatchV1(m) => {
                let tm_match = self.match_find(m).unwrap();
                let Some(config) = self
                    .db_read_only()
                    .tab_raw_server_config_v2()
                    .id()
                    .find(tm_match.get_active_config_id())
                else {
                    return Err("Cannot find config.".into());
                };
                self.db()
                    .event_raw_server_state()
                    .try_insert(EventRawServerState {
                        server_id,
                        config: config.config,
                        open: tm_match.is_open(),
                        occupied: true,
                        seamless,
                    })?;

                Ok(())
            }
            crate::competition::node::NodeHandle::ServerV1(s) => {
                let tm_server = self.server_find(s)?;
                let config = self
                    .db_read_only()
                    .tab_raw_server_config_v2()
                    .id()
                    .find(tm_server.get_config_id())
                    .unwrap();
                self.db()
                    .event_raw_server_state()
                    .try_insert(EventRawServerState {
                        server_id,
                        config: config.config,
                        open: tm_server.is_open(),
                        occupied: true,
                        seamless,
                    })?;

                Ok(())
            }
            _ => {
                log::error!(
                    "Requested a configuration from a node type other than Match or Server?"
                );
                Err("Requested a configuration from a node type other than Match or Server?".into())
            }
        }
    }
}

// How could we archieve adaptive configuration that changes dynamically with players???
// How could we filter the players better?

// -> Yoink 16 -> Yoink 16.
// How would we implement a conditional node????

#[reducer]
fn raw_server_config_shared_new(
    ctx: &ReducerContext,
    competition_id: u32,
    config: ServerConfigV2,
) -> Result<(), String> {
    ctx.auth_builder(competition_id)
        //.permission(CompetitionPermissionsV1::TODO)
        .authorize()?;

    ctx.raw_server_config_new(config, competition_id)?;

    Ok(())
}

#[reducer]
fn raw_server_config_shared_update(
    ctx: &ReducerContext,
    config_id: u32,
    config: ServerConfigV2,
) -> Result<(), String> {
    let Some(raw_config) = ctx
        .db_read_only()
        .tab_raw_server_config_v2()
        .id()
        .find(config_id)
    else {
        return Err("Config not found.".into());
    };

    ctx.auth_builder(raw_config.competition_id)
        //.permission(CompetitionPermissionsV1::TODO)
        .authorize()?;

    ctx.raw_server_config_update(config_id, config)?;

    Ok(())
}

#[reducer]
fn unstable_raw_server_config_update_maps(
    ctx: &ReducerContext,
    config_id: u32,
    maps: Vec<String>,
) -> Result<(), String> {
    let Some(mut raw_config) = ctx
        .db_read_only()
        .tab_raw_server_config_v2()
        .id()
        .find(config_id)
    else {
        return Err("Config not found.".into());
    };

    //TODO
    /* ctx.auth_builder(raw_config.competition_id)
    //.permission(CompetitionPermissionsV1::TODO)
    .authorize()?; */

    raw_config.config.set_maps(maps);

    ctx.db.tab_raw_server_config_v2().id().update(raw_config);

    Ok(())
}

//TODO also handle upwards nesting because shared configs should be inherited.
#[procedure(name = "unstable_competition_raw_server_config_v2")]
fn unstable_competition_raw_server_config_v2(
    ctx: &mut ProcedureContext,
    competition_id: u32,
) -> Vec<RawServerConfigV2> {
    ctx.with_tx(|ctx| {
        ctx.db
            .tab_raw_server_config_v2()
            .competition_id()
            .filter(competition_id)
    })
    .collect()
}

mod migrate {
    use spacetimedb::{ReducerContext, Table, reducer};

    use crate::{
        auto_inc_manual::AutoIncWrite,
        raw_server::config::{
            RawServerConfigV2, tab_raw_server_config, tab_raw_server_config_v2,
            tab_raw_server_config_v2__TableHandle,
        },
    };

    #[reducer]
    fn migration_raw_server_config_to_v2(ctx: &ReducerContext) -> Result<(), String> {
        if ctx.db.tab_raw_server_config_v2().count() != 0 {
            return Err("The table is not empty anymore.".into());
        }
        let rows = ctx.db.tab_raw_server_config().iter();
        let mut max_id = 0;
        for row in rows {
            if row.id > max_id {
                max_id = row.id
            }
            ctx.db
                .tab_raw_server_config_v2()
                .try_insert(RawServerConfigV2 {
                    id: row.id,
                    competition_id: row.competition_id,
                    config: row.config.into(),
                })?;
        }

        ctx.auto_inc_migration::<tab_raw_server_config_v2__TableHandle>(max_id);

        Ok(())
    }
}
