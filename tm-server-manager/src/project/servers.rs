/* use spacetimedb::{ReducerContext, Table, ViewContext, reducer, table, view};

use crate::{
    authorization::Authorization, competition::CompetitionPermissionsV1, raw_server::{
        RawServerV1, tab_raw_server, tab_raw_server__view, tab_raw_server_occupation__view,
        user_raw_server_pool,
    }
};

#[table(accessor=tab_project_server)]
pub struct ProjectServer {
    #[index(hash)]
    pub project_id: u32,
    #[index(hash)]
    pub server_id: u32,
}

/// Allows a user to lend a server he owns to a project.
#[reducer]
fn lend_raw_server(ctx: &ReducerContext, server_id: u32, project_id: u32) -> Result<(), String> {
    let user_account = ctx.get_user_account()?;
    ctx.auth_builder(project_id, user_account)?
        .permission(CompetitionPermissionsV1::RAW_SERVER_ADD)
        .authorize()?;

    let Some(server) = ctx.db.tab_raw_server().id().find(project_id) else {
        return Err("Server not found".into());
    };

    if server.account_id != user_account {
        return Err("Not the owner of the server!".into());
    }

    if ctx
        .db
        .tab_project_server()
        .server_id()
        .filter(server_id)
        .any(|s| s.project_id == project_id)
    {
        return Err("Server was already lended to project.".into());
    }

    ctx.db.tab_project_server().try_insert(ProjectServer {
        project_id,
        server_id,
    })?;

    Ok(())
}

/// Allows a user to delete a server.
/// This can be either the owner of the server or an authorized project member
#[reducer]
fn revoke_raw_server(ctx: &ReducerContext, server_id: u32, project_id: u32) -> Result<(), String> {
    let user_account = ctx.get_user_account()?;

    let Some(server) = ctx.db.tab_raw_server().id().find(project_id) else {
        return Err("Server not found".into());
    };

    // If the server owner requests a deletion it always passes.
    if server.account_id == user_account {
        ctx.db.tab_project_server().delete(ProjectServer {
            project_id,
            server_id,
        });
        return Ok(());
    }

    ctx.auth_builder(project_id, user_account)?
        .permission(CompetitionPermissionsV1::RAW_SERVER_REVOKE)
        .authorize()?;

    ctx.db.tab_project_server().delete(ProjectServer {
        project_id,
        server_id,
    });

    Ok(())
}

/// The Raw server pool are all servers of an account which are verified.
#[view(accessor= project_available_server_pool, public)]
pub(crate) fn project_available_server_pool(
    ctx: &ViewContext, /* project_id: u32 */
) -> Vec<RawServerV1> {
    let project_id = 1u32; //TODO replace with arg
    let Ok(account_id) = ctx.get_user_account() else {
        return Vec::new();
    };
    //TODO which perissino should we use for this??
    //ctx.auth_builder(project_id, account_id)?.permission(ProjectPermissionsV1::SER)

    ctx.db
        .tab_project_server()
        .project_id()
        .filter(project_id)
        .filter_map(|s| {
            let server = ctx.db.tab_raw_server().id().find(s.server_id).unwrap();
            if !server.is_verified() {
                None
            } else {
                if ctx
                    .db
                    .tab_raw_server_occupation()
                    .server_id()
                    .find(server.id)
                    .is_some()
                {
                    return None;
                }
                Some(server)
            }
        })
        .collect()
}
 */