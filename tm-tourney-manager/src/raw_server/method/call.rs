use spacetimedb::{ReducerContext, Table, reducer};
use tm_server_types::method::MethodCall;

use crate::{auth::Authorization, raw_server::tab_raw_server_online};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tm_server_method_call, public))]
pub struct TmServerMethodCall {
    #[primary_key]
    #[auto_inc]
    pub id: u32,

    server_id: String,

    account_id: String,

    method: MethodCall,
}

impl TmServerMethodCall {
    pub(crate) fn get_server(&self) -> &String {
        &self.server_id
    }
}

#[reducer]
pub fn server_method_call(
    ctx: &ReducerContext,
    server_id: String,
    method: MethodCall,
) -> Result<(), String> {
    let account_id = ctx.get_user()?;

    if ctx
        .db
        .tab_raw_server_online()
        .tm_login()
        .find(&server_id)
        .is_some()
    {
        return Err(format!(
            "Server with id {server_id} was not found or is not online."
        ));
    };

    ctx.db
        .tm_server_method_call()
        .try_insert(TmServerMethodCall {
            id: 0,
            account_id,
            server_id,
            method,
        })?;

    Ok(())
}
