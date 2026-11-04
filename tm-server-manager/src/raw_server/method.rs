use std::time::SystemTime;

use spacetimedb::{
    CtxWithTimestamp, Local, ReducerContext, Table, TimeDuration, Timestamp, Uuid, ViewContext,
    reducer, table, view,
};
use tm_server_types::method::{MethodCall, MethodResponse};

use crate::{
    authorization::Authorization,
    raw_server::{tab_raw_server, tab_raw_server__view},
};

#[table(accessor=tab_raw_server_method, vis_private)]
struct RawServerMethod {
    call_time: Timestamp,
    response_time: Timestamp,
    #[primary_key]
    #[auto_inc]
    id: u32,

    #[index(hash)]
    server_id: u32,

    user_id: u32,

    call: MethodCall,
    resposne: MethodResponse,
}

#[table(accessor=event_raw_server_method, vis_private,public,event)]
struct EventRawServerMethod {
    #[primary_key]
    id: u32,

    #[index(hash)]
    server_id: u32,

    call: MethodCall,
}

impl RawServerMethod {
    pub(crate) fn get_server(&self) -> u32 {
        self.server_id
    }
}

#[reducer]
fn server_method_call(
    ctx: &ReducerContext,
    server_login: String,
    call: MethodCall,
) -> Result<(), String> {
    let user_id = ctx.user_id()?;

    //TODO permissions.

    let Some(server) = ctx.db.tab_raw_server().server_login().find(&server_login) else {
        return Err(format!(
            "Server with id {server_login} was not found or is not online."
        ));
    };

    let method = ctx.db.tab_raw_server_method().try_insert(RawServerMethod {
        id: 0,
        user_id,
        call_time: ctx.timestamp,
        response_time: Timestamp::from_time_duration_since_unix_epoch(TimeDuration::ZERO),
        server_id: server.id,
        call,
        resposne: MethodResponse::Pending,
    })?;

    ctx.db
        .event_raw_server_method()
        .try_insert(EventRawServerMethod {
            id: method.id,
            server_id: method.server_id,
            call: method.call,
        })?;

    Ok(())
}

#[reducer]
fn server_method_response(
    ctx: &ReducerContext,
    call_id: u32,
    response: MethodResponse,
) -> Result<(), String> {
    let server_id = ctx.server_id()?;

    let Some(mut method) = ctx.db.tab_raw_server_method().id().find(call_id) else {
        return Err(format!(
            "Cannot respond to nen existent MethodCall. id: {call_id} was not found."
        ));
    };

    if server_id != method.get_server() {
        return Err("Different server responded to the method call. Aborting".into());
    }

    method.response_time = ctx.timestamp;
    method.resposne = response;

    Ok(())
}

pub(crate) trait RawServerMethodWrite {
    fn send_raw_server_message(
        &self,
        server_id: u32,
        user_id: u32,
        message: String,
    ) -> Result<(), String>
    where
        Self: CtxWithTimestamp;
}

impl<Db: spacetimedb::CtxDbWrite> RawServerMethodWrite for Db {
    fn send_raw_server_message(
        &self,
        server_id: u32,
        user_id: u32,
        message: String,
    ) -> Result<(), String>
    where
        Self: spacetimedb::CtxWithTimestamp,
    {
        let Some(server) = self.db().tab_raw_server().id().find(server_id) else {
            return Err(format!(
                "Server with id {server_id} was not found or is not online."
            ));
        };

        let method = self
            .db()
            .tab_raw_server_method()
            .try_insert(RawServerMethod {
                id: 0,
                user_id,
                call_time: self.timestamp(),
                response_time: Timestamp::from_time_duration_since_unix_epoch(TimeDuration::ZERO),
                server_id: server.id,
                call: MethodCall::ChatSendServerMessage(message),
                resposne: MethodResponse::Pending,
            })?;

        self.db()
            .event_raw_server_method()
            .try_insert(EventRawServerMethod {
                id: method.id,
                server_id: method.server_id,
                call: method.call,
            })?;

        Ok(())
    }
}

/* //TODO eval if this can be done with event table
#[view(accessor= raw_server_method_call,public)]
fn raw_server_method_call(ctx: &ViewContext) -> Vec<RawServerMethodLog> {
    let Some(server) = ctx.db.tab_raw_server().identity().find(ctx.sender()) else {
        return Vec::new();
    };
    ctx.db
        .tab_raw_server_method_call()
        .server_id()
        .filter(server.id)
        .collect()
} */
