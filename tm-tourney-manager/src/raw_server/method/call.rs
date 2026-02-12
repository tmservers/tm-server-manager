use spacetimedb::{
    Query, ReducerContext, Table, Timestamp, Uuid, ViewContext, reducer, table, view,
};
use tm_server_types::method::MethodCall;

use crate::{
    authorization::Authorization,
    raw_server::{tab_raw_server, tab_raw_server__view},
};

#[table(name=tab_raw_server_method_call)]
#[table(name=tab_raw_server_method_call_resolved)]
pub struct RawServerMethodCall {
    #[primary_key]
    #[auto_inc]
    pub id: u32,

    #[index(hash)]
    server_login: String,

    account_id: Uuid,

    timestamp: Timestamp,

    method: MethodCall,
}

impl RawServerMethodCall {
    pub(crate) fn get_server(&self) -> &String {
        &self.server_login
    }
}

#[reducer]
pub fn server_method_call(
    ctx: &ReducerContext,
    server_login: String,
    method: MethodCall,
) -> Result<(), String> {
    let account_id = ctx.get_user()?.account_id;

    if ctx
        .db
        .tab_raw_server()
        .server_login()
        .find(&server_login)
        .is_some()
    {
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
            server_login,
            method,
        })?;

    Ok(())
}

//TODO eval if this can be done with event table
#[view(name= raw_server_method_call,public)]
fn raw_server_method_call(ctx: &ViewContext) -> Vec<RawServerMethodCall> {
    let Some(server) = ctx.db.tab_raw_server().identity().find(ctx.sender) else {
        return Vec::new();
    };
    ctx.db
        .tab_raw_server_method_call()
        .server_login()
        .filter(&server.server_login)
        //.map(|v| v.method)
        .collect()
}
