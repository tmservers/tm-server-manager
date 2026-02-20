use spacetimedb::{ReducerContext, Table, reducer, table};
use tm_server_types::method::{self, MethodResponse};

use crate::{
    authorization::Authorization,
    raw_server::method::call::{tab_raw_server_method_call, tab_raw_server_method_call_resolved},
};

#[table(name=tab_raw_server_method_response)]
pub struct TmServerMethodResponse {
    #[primary_key]
    pub id: u32,

    response: MethodResponse,
}

#[reducer]
pub fn server_method_response(
    ctx: &ReducerContext,
    call_id: u32,
    response: MethodResponse,
) -> Result<(), String> {
    //TODO do the proper auth
    // the challenge is who is able to access the server? not only owner i guess

    let server = ctx.get_server()?;

    let Some(method_call) = ctx.db.tab_raw_server_method_call().id().find(call_id) else {
        return Err(format!(
            "Cannot respond to nen existent MethodCall. id: {call_id} was not found."
        ));
    };

    if server.id != method_call.get_server() {
        return Err("Different server responded to the method call. Aborting".into());
    }

    ctx.db
        .tab_raw_server_method_response()
        .try_insert(TmServerMethodResponse {
            id: call_id,
            response,
        })?;

    if !ctx.db.tab_raw_server_method_call().id().delete(call_id) {
        return Err(format!(
            "Deletion of the method call failed respond to nen existent MethodCall. id: {call_id} was not found."
        ));
    };

    ctx.db
        .tab_raw_server_method_call_resolved()
        .try_insert(method_call)?;

    Ok(())
}
