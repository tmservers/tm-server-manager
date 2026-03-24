use spacetimedb::{
    AnonymousViewContext, DbContext, Local, ReducerContext, Table, ViewContext, reducer, table,
    view,
};

use crate::{
    authorization::Authorization,
    competition::CompetitionPermissionsV1,
    raw_server::{
        RawServerV1, occupation::TabRawServerOccupationRead, tab_raw_server, tab_raw_server__view,
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
fn lend_raw_server(
    ctx: &ReducerContext,
    server_id: u32,
    competition_id: u32,
) -> Result<(), String> {
    let account_id = ctx
        .auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::RAW_SERVER_ADD)
        .authorize()?;

    let Some(server) = ctx.db.tab_raw_server().id().find(competition_id) else {
        return Err("Server not found".into());
    };

    if server.user_account_id != account_id {
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
fn revoke_raw_server(
    ctx: &ReducerContext,
    server_id: u32,
    competition_id: u32,
) -> Result<(), String> {
    let user_account = ctx.get_user_account()?;

    let Some(server) = ctx.db.tab_raw_server().id().find(competition_id) else {
        return Err("Server not found".into());
    };

    // If the server owner requests a deletion it always passes.
    if server.user_account_id == user_account {
        ctx.db
            .tab_competition_raw_server()
            .delete(CompetitionServer {
                competition_id,
                server_id,
            });
        return Ok(());
    }

    ctx.auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::RAW_SERVER_REVOKE)
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
fn competition_available_server_pool(
    ctx: &ViewContext, /* competition_id: u32 */
) -> Vec<RawServerV1> {
    let competition_id = 1u32; //TODO replace with arg
    let Ok(account_id) = ctx.get_user_account() else {
        return Vec::new();
    };
    //TODO which perissino should we use for this??
    //ctx.auth_builder(project_id, account_id)?.permission(ProjectPermissionsV1::SER)

    ctx.server_pool_available(competition_id)
}

pub(crate) trait TabCompetitionServerPoolRead {
    fn server_pool_available(&self, competition_id: u32) -> Vec<RawServerV1>;
}
pub(crate) trait TabCompetitionServerPoolWrite: TabCompetitionServerPoolRead {}

impl<Db: DbContext> TabCompetitionServerPoolRead for Db {
    fn server_pool_available(&self, competition_id: u32) -> Vec<RawServerV1> {
        //TODO recurse upwards to catch all inherited servers.

        self.db_read_only()
            .tab_competition_raw_server()
            .competition_id()
            .filter(competition_id)
            .filter_map(|s| {
                let server = self
                    .db_read_only()
                    .tab_raw_server()
                    .id()
                    .find(s.server_id)
                    .unwrap();
                if !server.is_verified() {
                    None
                } else {
                    if self.raw_server_is_occupied(server.id) {
                        return None;
                    }
                    Some(server)
                }
            })
            .collect()
    }
}

impl<Db: DbContext<DbView = Local>> TabCompetitionServerPoolWrite for Db {}
