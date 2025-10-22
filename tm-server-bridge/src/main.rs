use std::sync::OnceLock;

use nadeo_api::NadeoClient;

use spacetimedb_sdk::{DbContext, Error, Identity, Table, TableWithPrimaryKey};

use tm_tourney_manager_api_rs::*;

use tm_server_client::{ClientError, TrackmaniaServer, method::XmlRpcMethods};
use tokio::{signal, sync::Mutex};
use tracing::{info, instrument, warn};

use crate::{config::configure, state::sync, telemetry::init_tracing_subscriber};

mod config;
mod state;
mod telemetry;
#[cfg(test)]
mod test;

//TODO remove once authorization is there
const TM_SERVER_ID: &str = "test";

/// Exposes the associated trackmania server globally.
static TRACKMANIA: OnceLock<TrackmaniaServer> = OnceLock::new();

/// Exposes the SpacetimeDB connection.
static SPACETIME: OnceLock<DbConnection> = OnceLock::new();

/// Exposes the NadeoAPI with server auth.
static NADEO: OnceLock<Mutex<NadeoClient>> = OnceLock::new();

/// Path to the Filesystem of the trackmnia server UserData.
static TRACKMANIA_FILES: OnceLock<String> = OnceLock::new();

//static STATE: OnceLock<Mutex<State>> = OnceLock::new();

/// Load credentials from a file and connect to the database.
#[instrument(level = "debug")]
fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        .on_connect_error(on_connect_error)
        .on_disconnect(on_disconnected)
        .with_module_name(
            std::env::var("SPACETIMEDB_MODULE").unwrap_or("tm-tourney-manager".to_string()),
        )
        .with_uri(std::env::var("SPACETIMEDB_URL").unwrap_or("http://localhost:1234".to_string()))
        .build()
        .expect("Failed to connect to SpacetimeDB. Aborting.")
}

/* struct State {
    nadeo: NadeoClient,
} */

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::from_path(env!("CARGO_MANIFEST_DIR").to_string() + "/.env").unwrap();

    // Tracing Guard.
    let _ = init_tracing_subscriber();

    let tm_user = std::env::var("TM_MASTERSERVER_LOGIN")
        .expect("Environment variable: TM_MASTERSERVER_LOGIN MUST be set");
    let tm_password = std::env::var("TM_MASTERSERVER_PASSWORD")
        .expect("Environment variable: TM_MASTERSERVER_password MUST be set");

    TRACKMANIA_FILES
        .set(std::env::var("TM_FILES").unwrap_or("./UserData".into()))
        .expect("The Path to the Trackmania Filesystem could not be established. Aborting.");

    {
        //Initialize the NadeoClient
        let nadeo = NadeoClient::builder()
            .with_server_auth(&tm_user, &tm_password)
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

    // Initial Configuration for the Trackmania server connection.
    {
        let server = TRACKMANIA.wait();

        let _: Result<bool, ClientError> = server.call("SetApiVersion", "2025-07-04").await;

        let _: Result<bool, ClientError> = server
            .call("Authenticate", ("SuperAdmin", "SuperAdmin"))
            .await;

        let _: Result<bool, ClientError> = server.call("EnableCallbacks", true).await;

        let _: Result<bool, ClientError> = server
            .call(
                "TriggerModeScriptEventArray",
                ("XmlRpc.SetApiVersion", ["3.11"]),
            )
            .await;

        let _: Result<bool, ClientError> = server
            .call(
                "TriggerModeScriptEventArray",
                ("XmlRpc.EnableCallbacks", ["true"]),
            )
            .await;

        warn!("{:?}", server.is_auto_save_replays_enabled().await);
        warn!("{:?}", server.auto_save_replays(true).await);
        warn!("{:?}", server.is_auto_save_replays_enabled().await);

        // Emit all events
        server.event(move |event| {
            let event = event.clone();
            tokio::spawn(async move {
                let server = TRACKMANIA.wait();
                if let tm_server_client::types::event::Event::EndRoundStart(info) = &event {
                    let file_name = format!("{}{}", info.count, info.time);
                    if let Err(error) = server.save_current_replay(&file_name).await {
                        tracing::error!(
                            "Failed to save Replay File after Round ended. Reason: {error}"
                        );
                        return;
                    };
                    let full_path = TRACKMANIA_FILES.wait().clone()
                        + "/Replays/"
                        + &std::env::var("TM_MASTERSERVER_LOGIN").unwrap()
                        + "/Autosaves/"
                        + &file_name
                        + ".Replay.Gbx";

                    match std::fs::read(&full_path) {
                        Ok(file) => {
                            tracing::error!(
                                "{:?} version: {}",
                                String::from_utf8(file[0..3].to_vec()),
                                u16::from_le_bytes(file[3..5].try_into().unwrap())
                            );

                            if let Err(error) = SPACETIME.wait().reducers.post_ghost(file) {
                                tracing::error!(
                                    "Failed to add Ghosts for current round. Reason {error}"
                                )
                            };
                            //TODO parse the file and split off ghosts?
                            // Open questiones:
                            // - What about time attack?
                            //   - No Ghost can be associated with a player but rather a Vec<Ghost> for all the runs
                            // - How much file size savings when extracting ghosts.
                            // - Should the transformation happen on the client or the server?
                            //   - Probably client side since we already have a sidecar anyway...
                            // - Which metadata should be stored alongside the ghost? (can happen server side)
                        }
                        Err(error) => {
                            tracing::error!("Failed to read replay file. Reason: {error}")
                        }
                    };
                    if let Err(error) = std::fs::remove_file(&full_path) {
                        tracing::error!("Failed to delete the current replay file! Reason: {error}")
                    };
                }

                let spacetime = SPACETIME.wait();
                if spacetime
                    .reducers
                    .post_event(
                        TM_SERVER_ID.into(),
                        //SAFETY: Its the same type. Sadly Rust can not know that :< .
                        unsafe {
                            std::mem::transmute::<
                                tm_server_client::types::event::Event,
                                tm_tourney_manager_api_rs::Event,
                            >(event)
                        },
                    )
                    .is_err()
                {
                    println!("Event failed to publish!")
                }
            });
        })
    }

    // Connect to SpacetimeDB
    {
        let spacetime = connect_to_db();
        _ = SPACETIME.set(spacetime);
    }

    // Initialize state subscriptions for the server.
    {
        let spacetime = SPACETIME.wait();

        _ = spacetime
            .subscription_builder()
            .on_applied(|_| tracing::debug!("Subscription successfully applied!"))
            .on_error(|_, mhm| tracing::error!("Subscription failed: {mhm:?}"))
            .subscribe(format!(
                "SELECT * FROM tm_server WHERE id = '{TM_SERVER_ID}'"
            ));

        _ = spacetime.reducers.add_server(TM_SERVER_ID.into());

        spacetime.db.tm_server().on_insert(server_bootstrap);
        spacetime.db.tm_server().on_update(server_update);
    }

    tokio::spawn(async move {
        loop {
            _ = SPACETIME.wait().run_async().await;
        }
    });

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }

    Ok(())
}

/// Our `on_connect_error` callback: print the error, then exit the process.
fn on_connect_error(_ctx: &ErrorContext, err: Error) {
    eprintln!("SpacetimeDB connection error: {:?}", err);
    std::process::exit(1);
}

/// Our `on_disconnect` callback: print a note, then exit the process.
fn on_disconnected(_ctx: &ErrorContext, err: Option<Error>) {
    if let Some(err) = err {
        eprintln!("Disconnected from SpacetimeDB: {}", err);
        std::process::exit(1);
    } else {
        println!("Disconnected.");
        std::process::exit(0);
    }
}

fn server_update(_: &EventContext, old: &TmServer, new: &TmServer) {
    let local_server = TRACKMANIA.wait();

    let new = new.clone();
    let old = old.clone();

    tokio::spawn(async move {
        if let Some(method) = new.server_method {
            let _: Result<bool, ClientError> = local_server
                .call("ChatSendServerMessage", "Method called")
                .await;
        }
        if old.config != new.config {
            configure(new).await;
        }

        //server.method(method)
        /* let _: Result<bool, ClientError> = server
        .call(
            "TriggerModeScriptEventArray",
            (
                "Maniaplanet.Pause.SetActive",
                [if paused { "true" } else { "false" }],
            ),
        )
        .await; */
    });
}

fn server_bootstrap(ctx: &EventContext, new: &TmServer) {
    let local_server = TRACKMANIA.wait();
    let new = new.clone();
    tokio::spawn(async move {
        let _: Result<bool, ClientError> = local_server
            .call(
                "ChatSendServerMessage",
                "[tm-server-bridge] Bootstrapping the server!",
            )
            .await;

        configure(new).await;
        sync().await;

        let _: Result<bool, ClientError> = local_server
            .call(
                "ChatSendServerMessage",
                "[tm-server-bridge] Bootstrapping successfull :>",
            )
            .await;

        /* local_server
           .send_display_manialink_page(
               r#"<?xml version="1.0" encoding="utf-8" standalone="yes" ?>
<manialink version="3">
    <label text="Custom UI! owo" />
    <quad image="https://www.waddensea-worldheritage.org/sites/default/files/styles/main_image_landscape_crop/public/20-11-09_habour%20seals%20report_TTF_5200_0.JPG?itok=W1eZAlLm" autoscale="1" size="80 45" keepration="Fit"/>
</manialink>"#,
               20000,
               false,
           )
           .await;
        */
        //local_server.ui_modules_get_properties().await
    });
}
