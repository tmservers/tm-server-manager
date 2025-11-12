use tm_server_client::types::method::{MethodCall, MethodResponse};
use tm_tourney_manager_api_rs::{EventContext, TmServerMethodCall, server_method_response};

use crate::{SPACETIME, TRACKMANIA};

pub fn method_call_received(_: &EventContext, method: &TmServerMethodCall) {
    tracing::error!("{method:#?}");
    let new = method.clone();
    tokio::spawn(async move {
        let response = TRACKMANIA
            .wait()
            .method(
                //SAFETY: Its the same type but rust cant know that.
                unsafe {
                    std::mem::transmute::<tm_tourney_manager_api_rs::MethodCall, MethodCall>(
                        new.method,
                    )
                },
            )
            .await;

        SPACETIME.wait().reducers.server_method_response(
            new.id, //SAFETY: Its the same type but rust cant know that.
            unsafe {
                std::mem::transmute::<MethodResponse, tm_tourney_manager_api_rs::MethodResponse>(
                    response,
                )
            },
        )
    });
}
