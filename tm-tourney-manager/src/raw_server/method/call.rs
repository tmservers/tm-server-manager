use spacetimedb::{
    Query, ReducerContext, Table, Timestamp, Uuid, ViewContext, reducer, table, view,
};
use tm_server_types::method::MethodCall;

use crate::{
    authorization::Authorization,
    raw_server::{tab_raw_server, tab_raw_server__view},
};

#[table(accessor=tab_raw_server_method_call)]
#[table(accessor=tab_raw_server_method_call_resolved)]
pub struct RawServerMethodCall {
    #[primary_key]
    #[auto_inc]
    pub id: u32,

    #[index(hash)]
    server_id: u32,

    account_id: Uuid,

    timestamp: Timestamp,

    method: MethodCall,
}

impl RawServerMethodCall {
    pub(crate) fn get_server(&self) -> u32 {
        self.server_id
    }
}

#[reducer]
pub fn server_method_call(
    ctx: &ReducerContext,
    server_login: String,
    method: MethodCall,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;

    let Some(server) = ctx.db.tab_raw_server().server_login().find(&server_login) else {
        return Err(format!(
            "Server with id {server_login} was not found or is not online."
        ));
    };

    ctx.db
        .tab_raw_server_method_call()
        .try_insert(RawServerMethodCall {
            id: 0,
            account_id,
            timestamp: ctx.timestamp,
            server_id: server.id,
            method,
        })?;

    Ok(())
}

//TODO eval if this can be done with event table
#[view(accessor= raw_server_method_call,public)]
fn raw_server_method_call(ctx: &ViewContext) -> Vec<RawServerMethodCall> {
    let Some(server) = ctx.db.tab_raw_server().identity().find(ctx.sender()) else {
        return Vec::new();
    };
    ctx.db
        .tab_raw_server_method_call()
        .server_id()
        .filter(server.id)
        .collect()
}
