use std::sync::OnceLock;

use nadeo_api::NadeoClient;

use spacetimedb_sdk::{DbContext, Error, Table, Uuid};

use tm_tourney_manager_api_rs::*;

use tm_server_controller::{
    TrackmaniaServer,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tokio::{signal, sync::Mutex};
use tracing::{instrument, warn};

use crate::{
    chat::setup_chat, config::config_update, methods::method_call_received,
    state::setup_state_synchronization, telemetry::init_tracing_subscriber,
};

mod chat;
mod config;
mod methods;
mod state;
mod telemetry;

#[cfg(test)]
mod test;

/// Exposes the associated trackmania server globally.
static TRACKMANIA: OnceLock<TrackmaniaServer> = OnceLock::new();

/// Exposes the SpacetimeDB connection.
static SPACETIME: OnceLock<DbConnection> = OnceLock::new();

/// Exposes the NadeoAPI with server auth.
static NADEO: OnceLock<Mutex<NadeoClient>> = OnceLock::new();

/// Path to the Filesystem of the trackmnia server UserData.
static TRACKMANIA_FILES: OnceLock<String> = OnceLock::new();

static SERVER_CONFIG: OnceLock<ServerConfig> = OnceLock::new();

//static STATE: OnceLock<Mutex<State>> = OnceLock::new();

/// Load credentials from a file and connect to the database.
#[instrument(level = "debug")]
fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        .on_connect_error(on_connect_error)
        .on_disconnect(on_disconnected)
        .with_database_name(std::env::var("SPACETIMEDB_MODULE").unwrap_or("tmservers".to_string()))
        .with_uri(
            std::env::var("SPACETIMEDB_URL")
                .unwrap_or("https://maincloud.spacetimedb.com".to_string()),
        )
        .build()
        .expect("Failed to connect to SpacetimeDB. Aborting.")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::from_path(env!("CARGO_MANIFEST_DIR").to_string() + "/.env")
        .expect("Es konnte kein .env file gefunden werden!");

    // Tracing Guard.
    let _ = init_tracing_subscriber();

    let tm_server_login = std::env::var("TM_MASTERSERVER_LOGIN")
        .expect("Environment variable: TM_MASTERSERVER_LOGIN MUST be set");
    let tm_server_password = std::env::var("TM_MASTERSERVER_PASSWORD")
        .expect("Environment variable: TM_MASTERSERVER_password MUST be set");
    let tm_account_id = std::env::var("TM_ACCOUNT_ID")
        .expect("Environment variable: TM_ACCOUNT_ID MUST be set at the moment.
        This will be the account where the server will be available under and can be obtained from e.g. trackmania.io. 
        We hope to make this optional in the future but depend on a change from nadeo on that sooo good luck ^^");

    TRACKMANIA_FILES
        .set(std::env::var("TM_FILES").unwrap_or("./UserData".into()))
        .expect("The Path to the Trackmania Filesystem could not be established. Aborting.");

    {
        //Initialize the NadeoClient
        let nadeo = NadeoClient::builder()
            .with_server_auth(&tm_server_login, &tm_server_password)
            .user_agent("tm-server-bridge")
            .build()
            .await
            .unwrap();
        _ = NADEO.set(nadeo.into());
    }

    //Connect to the Trackmania server
    {
        let server = TrackmaniaServer::new("127.0.0.1:5001").await;
        _ = TRACKMANIA.set(server);
    }

    // Connect to SpacetimeDB
    {
        let spacetime = connect_to_db();
        let tm_account_id = Uuid::parse_str(&tm_account_id)?;

        _ = SPACETIME.set(spacetime);

        tokio::spawn(async move {
            let connection = SPACETIME.wait();
            loop {
                _ = connection.run_async().await;
            }
        });

        //TODO proper error handling
        SPACETIME.wait().procedures.login_as_server_then(
            tm_server_login,
            tm_server_password,
            tm_account_id,
            |_, result| {
                if result.is_ok() && result.unwrap().is_ok() {
                    tracing::info!("Successfully connected to tmservers.live!")
                } else {
                    tracing::error!("Could not successfully authenticate as a server!");
                    std::process::exit(1)
                };
            },
        );
    }

    // Initial Configuration for the Trackmania server connection.
    {
        let server = TRACKMANIA.wait();

        let _: bool = server.call("SetApiVersion", "2025-07-04").await?;

        server.authenticate("SuperAdmin", "SuperAdmin").await?;

        let _: bool = server
            .call(
                "TriggerModeScriptEventArray",
                ("XmlRpc.SetApiVersion", ["3.11"]),
            )
            .await?;

        server.enable_callbacks(true).await?;
        server.enable_mode_script_callbacks(true).await?;

        server.chat_manual_routing(true, false).await?;

        //TODO remove
        /*  _ = server.get_callbacks_list_disabled().await?;
        _ = server.get_callbacks_list().await?; */

        /* server.on_event(|event| {
            let event = event.clone();
            tokio::spawn(async move {
                let server = TRACKMANIA.wait();

                //TODO reenable automatic kicking
                if let tm_server_controller::event::Event::PlayerConenct(player) = &event
                    && player.account_id != "bla"
                    && !player.is_spectator
                {
                    _ = server
                        .kick(
                            player.account_id.clone(),
                            Some("You are not on the whitelist! :("),
                        )
                        .await;
                    tracing::error!("player successfully kicked {}", &player.account_id);
                };
            });
        }) */
    }

    setup_state_synchronization().await;
    setup_chat().await;

    // Initialize state subscriptions for the server.
    {
        let spacetime = SPACETIME.wait();
        _ = spacetime
            .subscription_builder()
            .on_applied(|_| tracing::debug!("Subscription successfully applied!"))
            .on_error(|_, mhm| tracing::error!("Subscription failed: {mhm:?}"))
            .add_query(|ctx| ctx.from.raw_server_method_call().build())
            .add_query(|ctx| ctx.from.raw_server_config().build())
            .subscribe();

        //TODO switch to this_server if on_update callbacks are there
        spacetime.db.raw_server_config().on_insert(config_update);

        spacetime
            .db
            .raw_server_method_call()
            .on_insert(method_call_received);
    }

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            tracing::error!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }

    Ok(())
}

/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(_ctx: &ErrorContext, err: Error) {
    tracing::error!("SpacetimeDB connection error: {:?}", err);
    std::process::exit(1);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &ErrorContext, err: Option<Error>) {
    if let Some(err) = err {
        tracing::error!("Disconnected from SpacetimeDB: {}", err);
        std::process::exit(1);
    } else {
        tracing::error!("Disconnected.");
        std::process::exit(0);
    }
}
