use spacetimedb::{ReducerContext, Table, reducer};
use tm_server_types::method::MethodCall;

use crate::server::tm_server;

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tm_server_method_call, public))]
pub struct TmServerMethodCall {
    #[primary_key]
    #[auto_inc]
    pub id: u32,

    server_id: String,
    //Audit Log?
    user_id: String,

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
    //TODO do the proper auth
    // the challenge is who is able to access the server? not only owner i guess

    let Some(tm_server) = ctx.db.tm_server().id().find(&server_id) else {
        return Err(format!("Server with id {server_id} was not found."));
    };

    if !tm_server.online {
        return Err(format!("Server with id {server_id} is not online."));
    }

    ctx.db
        .tm_server_method_call()
        .try_insert(TmServerMethodCall {
            id: 0,
            //TODO get from user auth token
            user_id: "test_user".into(),
            server_id,
            method,
        })?;

    Ok(())
}
