use spacetimedb::{CaseConversionPolicy, ReducerContext, Table, Uuid};

use crate::{
    raw_server::tab_raw_server,
    user::{
        UserIdentity, UserIdsMap, UserV1 as UserStruct, tab_user as db_user, tab_user_identity,
        tab_user_ids_map,
    },
};

pub mod authorization;
pub mod competition;
pub mod env;
pub mod maps;
pub mod monitoring;
pub mod portal;
pub mod project;
pub mod raw_server;
pub mod record;
pub mod registration;
pub mod schedule;
pub mod tm_match;
pub mod tm_server;
pub mod user;
pub mod worker;

// This is to avoid the enum variants to become camelCase
#[spacetimedb::settings]
const CASE_CONVERSION_POLICY: CaseConversionPolicy = CaseConversionPolicy::None;

#[derive(serde::Deserialize)]
struct SpacetimeAuthClaims {
    preferred_username: String,
    login_method: String,
    // Trackmania account_id
    provider_id: String,
}

#[spacetimedb::reducer(client_connected)]
fn client_connected(ctx: &ReducerContext) -> Result<(), String> {
    // If someone tries to connect with a token it needs to be a token from SpacetimeAuth
    // with the Trackmania provider. Otherwise you should connect annonymously.
    if let Some(jwt) = ctx.sender_auth().jwt() {
        if jwt.issuer() == "localhost" {
            // Client connects annonymously.
            // Annonymous connections are used for:
            // - Servers
            // - Workers
            // - Read only general purpose applications and dont need full access for features.
            log::info!("Connected Annonymously");
            return Ok(());
        }
        // This is only that the batch scripts can run while developing.
        // The production feature flag is enforced in CI.
        #[cfg(not(feature = "production"))]
        if jwt.issuer() == "https://auth.spacetimedb.com" {
            log::warn!("Connected as test user Mr.Joermungandr in a development environment!");
            let account_id: Uuid = Uuid::parse_str("3467014a-c1cc-4aae-99fe-6beb5eca232a").unwrap();

            let preferred_username = String::from("Mr.Joermungandr");

            let user = ctx
                .db
                .tab_user()
                .account_id()
                .try_insert_or_update(UserStruct::new(account_id, preferred_username))?;
            ctx.db
                .tab_user_identity()
                .account_id()
                .try_insert_or_update(UserIdentity::new(account_id, ctx.sender()))?;
            ctx.db
                .tab_user_ids_map()
                .account_id()
                .try_insert_or_update(UserIdsMap::new(account_id, user.internal_id))?;

            return Ok(());
        }

        if jwt.issuer() == "https://auth.spacetimedb.com/oidc" {
            let claims = unsafe {
                json::from_str::<SpacetimeAuthClaims>(&mut jwt.raw_payload().to_string())
                    .map_err(|e| e.to_string())?
            };

            if claims.login_method != "trackmania" {
                return Err(format!(
                    "Invalid login_method in token. Cannot login with the {} provider.",
                    claims.login_method
                ));
            }

            let account_id = Uuid::parse_str(&claims.provider_id).map_err(|e| e.to_string())?;

            let user = ctx
                .db
                .tab_user()
                .account_id()
                .try_insert_or_update(UserStruct::new(account_id, claims.preferred_username))?;
            ctx.db
                .tab_user_identity()
                .account_id()
                .try_insert_or_update(UserIdentity::new(account_id, ctx.sender()))?;
            ctx.db
                .tab_user_ids_map()
                .account_id()
                .try_insert_or_update(UserIdsMap::new(account_id, user.internal_id))?;

            return Ok(());
        }

        Err("Tried to connect with the wrong issuer.".into())
    } else {
        //Internal
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
