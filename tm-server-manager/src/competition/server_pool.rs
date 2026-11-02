use spacetimedb::{ReducerContext, Table, ViewContext, reducer, table, view};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, CompetitionRead},
    raw_server::{
        CompetitionRawServer, RawServerV1, TabRawServerRead, occupation::TabRawServerOccupationRead,
    },
};

#[table(accessor=tab_competition_raw_server)]
pub struct CompetitionServer {
    #[index(hash)]
    pub competition_id: u32,
    #[index(hash)]
    pub server_id: u32,
}

/// Allows a user to lend a server he owns to a project.
#[reducer]
fn raw_server_add(ctx: &ReducerContext, server_id: u32, competition_id: u32) -> Result<(), String> {
    let user_id = ctx
        .auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::RAW_SERVER_ADD)
        .authorize()?;

    let server = ctx.raw_server_find(server_id)?;

    if server.user_id != user_id {
        return Err("Not the owner of the server!".into());
    }

    if ctx
        .db
        .tab_competition_raw_server()
        .server_id()
        .filter(server_id)
        .any(|s| s.competition_id == competition_id)
    {
        return Err("Server was already lended to project.".into());
    }

    ctx.db
        .tab_competition_raw_server()
        .try_insert(CompetitionServer {
            competition_id,
            server_id,
        })?;

    Ok(())
}

/// Allows a user to delete a server.
/// This can be either the owner of the server or an authorized project member
#[reducer]
fn raw_server_remove(
    ctx: &ReducerContext,
    server_id: u32,
    competition_id: u32,
) -> Result<(), String> {
    let user_id = ctx.user_id()?;

    let server = ctx.raw_server_find(server_id)?;

    // If the server owner requests a deletion it always passes.
    if server.user_id == user_id {
        ctx.db
            .tab_competition_raw_server()
            .delete(CompetitionServer {
                competition_id,
                server_id,
            });
        return Ok(());
    }

    ctx.auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::RAW_SERVER_REMOVE)
        .authorize()?;

    ctx.db
        .tab_competition_raw_server()
        .delete(CompetitionServer {
            competition_id,
            server_id,
        });

    Ok(())
}

/// The Raw server pool are all servers of an account which are verified.
#[view(accessor= competition_available_server_pool, public)]
fn competition_available_server_pool(ctx: &ViewContext, competition_id: u32) -> Vec<RawServerV1> {
    //TODO which permission should we use for this??
    if ctx
        .auth_builder(competition_id)
        //.permission(CompetitionPermissionsV1::OWNER)
        .authorize()
        .is_err()
    {
        return Vec::new();
    };

    ctx.server_pool_available(competition_id)
}

/// The servers lent directly to a competition, the ones of the competitions above are not included.
#[view(accessor = competition_raw_servers, public)]
fn competition_raw_servers(ctx: &ViewContext, competition_id: u32) -> Vec<CompetitionRawServer> {
    if ctx.auth_builder(competition_id).authorize().is_err() {
        return Vec::new();
    }

    ctx.db
        .tab_competition_raw_server()
        .competition_id()
        .filter(competition_id)
        .filter_map(|s| ctx.raw_server_find(s.server_id).ok())
        .map(|server| server.seen_from(ctx))
        .collect()
}

/// The competitions the servers of the user are lent to.
#[view(accessor = my_raw_server_lendings, public)]
fn my_raw_server_lendings(ctx: &ViewContext) -> Vec<CompetitionServer> {
    let Ok(user_id) = ctx.user_id() else {
        return Vec::new();
    };

    ctx.raw_servers_of_user(user_id)
        .into_iter()
        .flat_map(|server| ctx.db.tab_competition_raw_server().server_id().filter(server.id))
        .collect()
}

/* #[view(accessor= competition_occupied_server_pool, public)]
fn competition_occupied_server_pool(
    ctx: &ViewContext, /* competition_id: u32 */
) -> Vec<RawServerV1> {
    let competition_id = 1u32;

    //TODO

    Vec::new()
} */

pub(crate) trait TabCompetitionServerPoolRead {
    fn server_pool_available(&self, competition_id: u32) -> Vec<RawServerV1>;
}

impl<Db: spacetimedb::CtxDbRead> TabCompetitionServerPoolRead for Db {
    fn server_pool_available(&self, competition_id: u32) -> Vec<RawServerV1> {
        let tree = self.competition_ancestors(competition_id);

        let mut servers = Vec::new();
        for competition in tree {
            servers.extend(
                self.db_read_only()
                    .tab_competition_raw_server()
                    .competition_id()
                    .filter(competition)
                    .filter_map(|s| {
                        let server = self.raw_server_find(s.server_id).unwrap();
                        if !server.is_verified() || !server.is_online() {
                            None
                        } else {
                            if self.raw_server_is_occupied(server.id) {
                                return None;
                            }
                            Some(server)
                        }
                    }),
            )
        }

        servers
    }
}
