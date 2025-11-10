use spacetimedb::{ReducerContext, ReducerResult, Table, reducer, sats::algebraic_value::ser};
use tm_server_types::method::Method;

use crate::server::tm_server;

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tm_server_method, public))]
pub struct TmServerMethod {
    #[primary_key]
    #[auto_inc]
    id: u64,

    server_id: String,
    //Audit Log?
    user_id: String,

    method: Method,
}

#[reducer]
pub fn server_method_call(
    ctx: &ReducerContext,
    server_id: String,
    method: Method,
) -> Result<(), String> {
    //TODO do the proper auth
    // the challenge is who is able to access the server? not only owner i guess

    let Some(tm_server) = ctx.db.tm_server().id().find(&server_id) else {
        return Err(format!("Server with id {server_id} was not found."));
    };

    if !tm_server.online {
        return Err(format!("Server with id {server_id} is not online."));
    }

    ctx.db.tm_server_method().try_insert(TmServerMethod {
        id: 0,
        //TODO get from user auth token
        user_id: "test_user".into(),
        server_id,
        method,
    })?;

    Ok(())
}
