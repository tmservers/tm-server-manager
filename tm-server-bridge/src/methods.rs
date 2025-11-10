use tm_server_client::types::method::Method;
use tm_tourney_manager_api_rs::{EventContext, TmServerMethod};

use crate::TRACKMANIA;

pub fn method_call_received(_: &EventContext, method: &TmServerMethod) {
    tracing::error!("{method:#?}");
    let new = method.clone();
    tokio::spawn(async move {
        TRACKMANIA
            .wait()
            .method(
                //SAFETY: Its the same type but rust cant know that.
                unsafe {
                    std::mem::transmute::<tm_tourney_manager_api_rs::Method, Method>(new.method)
                },
            )
            .await;
    });
}
