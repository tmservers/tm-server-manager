use spacetimedb::{CaseConversionPolicy, ReducerContext, Table, Uuid};

use crate::{
    raw_server::tab_raw_server,
    user::{UserIdentity, UserV1 as UserStruct, tab_user as db_user, tab_user_identity},
};

pub mod authorization;
pub mod competition;
pub mod environment;
pub mod ghosts;
pub mod monitoring;
pub mod raw_server;
pub mod record;
pub mod registration;
pub mod scheduling;
pub mod tm_match;
pub mod tm_server;
pub mod project;
pub mod user;
pub mod worker;

// This is to avoid the enum variants to become camelCase
#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: CaseConversionPolicy = CaseConversionPolicy::None;

#[spacetimedb::reducer(client_connected)]
fn client_connected(ctx: &ReducerContext) -> Result<(), String> {
    // If someone tries to connect with a token it needs to be a token from SpacetimeAuth
    // with the Trackmania provider. Otherwise you should connect annonymously.
    if let Some(jwt) = ctx.sender_auth().jwt() {
        log::warn!("Tried to connect with jwt {}", jwt.raw_payload());

        //TODO get trackmania id claim.
        let account_id: Uuid = Uuid::parse_str("3467014a-c1cc-4aae-99fe-6beb5eca232a").unwrap();
        log::warn!("{account_id}");

        let preferred_username = String::from("Mr.Joermungandr");

        ctx.db
            .tab_user()
            .try_insert(UserStruct::new(account_id, preferred_username))?;
        ctx.db
            .tab_user_identity()
            .try_insert(UserIdentity::new(account_id, ctx.sender()))?;

        Ok(())
    } else {
        // Client connects annonymously.
        // Annonymous connections are used for:
        // - Servers
        // - Workers
        // - Read only general purpose applications and dont need full access for features.
        log::info!("Connected Annonymously");
        Ok(())
    }
}

#[spacetimedb::reducer(client_disconnected)]
fn client_disconnected(ctx: &ReducerContext) {
    if let Some(mut server) = ctx.db.tab_raw_server().identity().find(ctx.sender()) {
        server.set_offline();
        ctx.db.tab_raw_server().id().update(server);
    }
}
