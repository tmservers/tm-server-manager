use spacetimedb::{CaseConversionPolicy, Identity, ReducerContext, Uuid};

use crate::{
    authorization::Authorization,
    raw_server::TabRawServerWrite,
    user::{UserV1 as UserStruct, UserWrite},
};

pub mod authorization;
pub mod auto_inc_manual;
pub mod competition;
pub mod env;
pub mod input;
pub mod leaderboard;
pub mod maps;
pub mod monitoring;
pub mod output;
pub mod project;
pub mod raw_server;
pub mod record;
pub mod registration;
pub mod schedule;
pub mod tm_match;
pub mod tm_server;
pub mod user;

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
            use crate::user::UserWrite;
            if ctx.sender()
                == Identity::from_hex(
                    "c2007de22c53d985f0a30b8614f640dc56aded1401a230b3a48ae4c0d9a399e3",
                )
                .unwrap()
            {
                log::warn!("Connected as test user Marijntje04 in a development environment!");
                let account_id: Uuid =
                    Uuid::parse_str("bfcbe019-bc7f-4ee2-a405-a6c0ca7ee7b1").unwrap();

                let preferred_username = String::from("Marijntje04");
                let mut user = UserStruct::new(account_id);
                user.set_name(preferred_username);
                let user_id = ctx.user_insert(user)?;
                ctx.user_login(user_id, ctx.sender())?;
                return Ok(());
            } else {
                log::warn!("Connected as test user Mr.Joermungandr in a development environment!");
                let account_id: Uuid =
                    Uuid::parse_str("3467014a-c1cc-4aae-99fe-6beb5eca232a").unwrap();

                let preferred_username = String::from("Mr.Joermungandr");
                let mut user = UserStruct::new(account_id);
                user.set_name(preferred_username);
                let user_id = ctx.user_insert(user)?;
                ctx.user_login(user_id, ctx.sender())?;

                return Ok(());
            }
        }
        // This is done for the dashboard table subscription.
        #[cfg(feature = "production")]
        if jwt.issuer() == "https://auth.spacetimedb.com" {
            log::info!(
                "Connected with internal spacetime issuer. {}",
                jwt.subject()
            );
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

            let name = claims.preferred_username.clone();

            let mut user = UserStruct::new(account_id);
            user.set_name(claims.preferred_username);
            let user_id = ctx.user_insert(user)?;
            ctx.user_login(user_id, ctx.sender())?;

            log::info!("Account {name} with id {account_id} has connected.");

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
    if let Ok(server_id) = ctx.server_id() {
        ctx.raw_server_disconnected(server_id, ctx.timestamp);
    }
}
