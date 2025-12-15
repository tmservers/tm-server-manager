use std::sync::OnceLock;

use nadeo_api::NadeoClient;

use spacetimedb_sdk::{DbContext, Error, Identity, Table, TableWithPrimaryKey};

use takumi::{
    layout::{
        Viewport,
        node::{ContainerNode, NodeKind, TextNode},
    },
    rendering::RenderOptionsBuilder,
};
use tm_tourney_manager_api_rs::*;

use tm_server_client::{
    ClientError, TrackmaniaServer,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tokio::{signal, sync::Mutex};
use tracing::{info, instrument, warn};

use crate::{
    config::configure, methods::method_call_received, state::sync,
    telemetry::init_tracing_subscriber,
};

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

        let mhm = server.get_callbacks_list_disabled().await;
        let mhm = server.get_callbacks_list().await;

        // Emit all events
        server.event(move |event| {
            let event = event.clone();
            tokio::spawn(async move {
                let server = TRACKMANIA.wait();

                if let tm_server_client::types::event::Event::PlayerConenct(player) = &event
                    && player.login != "bla"
                    && !player.is_spectator
                {
                    _ = server
                        .kick(
                            player.login.clone(),
                            Some("You are not on the whitelist! :("),
                        )
                        .await;
                    tracing::error!("player successfully kicked {}", &player.login);
                };

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
                            SPACETIME.wait().procedures.post_round_replay(file);
                        }
                        Err(error) => {
                            tracing::error!("Failed to read replay file. Reason: {error}")
                        }
                    };
                    if let Err(error) = std::fs::remove_file(&full_path) {
                        tracing::error!("Failed to delete the current replay file! Reason: {error}")
                    };

                    //TODO remove
                    /* warn!(
                        "{:?},",
                        TRACKMANIA
                            .wait()
                            .set_player_points("NGcBSsHMSq6Z_mvrXsojKg".to_string(), 55)
                            .await
                    );
                    warn!(
                        "{:?},",
                        TRACKMANIA
                            .wait()
                            .set_player_points("iyOlLqb7TMmlOwxGwIdo-g".to_string(), 65)
                            .await
                    ); */
                }

                let spacetime = SPACETIME.wait();
                if spacetime
                    .reducers
                    .post_event(
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
            .subscribe([
                format!("SELECT * FROM tm_server WHERE id = '{tm_server_login}'"), //TODO replace with views
                "SELECT * FROM tm_server_method_call".into(), //TODO this should be possible with views since you should only be able to query the server as a server.
            ]);

        //TODO check if connecting has succeeded
        spacetime
            .procedures
            .promote_to_server(tm_server_login, tm_server_password, tm_account_id);

        spacetime.db.tm_server().on_insert(server_bootstrap);
        spacetime.db.tm_server().on_update(server_update);

        spacetime
            .db
            .tm_server_method_call()
            .on_insert(method_call_received);
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
    //TODO check for match status change aswell!

    if old.config != new.config {
        let new = new.clone();
        tokio::spawn(async move {
            configure(new).await;
        });
    }
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

        test_takumi_render_ui().await;

        //local_server.ui_modules_get_properties().await
    });
}

async fn test_takumi_render_ui() {
    /*  let node: NodeKind = NodeKind::Text(TextNode {
        style: None,
        text: "huh".into(),
        tw: None,
    });

    let render_context = takumi::GlobalContext::default();
    // Create a viewport
    let viewport = Viewport::new(Some(1200), Some(630));

    // Create render options
    let options = RenderOptionsBuilder::default()
        .viewport(viewport)
        .node(node)
        .global(&render_context)
        .build()
        .unwrap();

    // Render the layout to an `RgbaImage`
    let image = takumi::rendering::render(options).unwrap();

    TRACKMANIA.wait()
           .send_display_manialink_page(
               r#"<?xml version="1.0" encoding="utf-8" standalone="yes" ?>
<manialink version="3">
    <label text="Custom UI! owo" />
    <quad image="https://www.waddensea-worldheritage.org/sites/default/files/styles/main_image_landscape_crop/public/20-11-09_habour%20seals%20report_TTF_5200_0.JPG?itok=W1eZAlLm" autoscale="1" size="80 45" keepration="Fit"/>
</manialink>"#,
               20000,
               false,
           )
           .await; */
}
