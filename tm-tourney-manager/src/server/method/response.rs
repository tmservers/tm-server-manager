use spacetimedb::{ReducerContext, Table, reducer};
use tm_server_types::method::{self, MethodResponse};

use crate::{auth::Authorization, server::method::call::tm_server_method_call};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=tm_server_method_response, public))]
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

    let server_id = ctx.is_server()?;

    let Some(method_call) = ctx.db.tm_server_method_call().id().find(call_id) else {
        return Err(format!(
            "Cannot respond to nen existent MethodCall. id: {call_id} was not found."
        ));
    };

    if &server_id != method_call.get_server() {
        return Err("Different server responded to the method call. Aborting".into());
    }

    ctx.db
        .tm_server_method_response()
        .try_insert(TmServerMethodResponse {
            id: call_id,
            response,
        })?;

    Ok(())
}
