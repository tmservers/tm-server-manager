use std::{process::exit, sync::OnceLock};

use nadeo_api::NadeoClient;

use tm_server_controller::{
    TrackmaniaServer,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};

use crate::{chat::setup_chat, connection::MyDbConnection, state::setup_state_synchronization};
use tm_server_manager_api_rs::EventRawServerState;
use tokio::{signal, sync::Mutex};

mod chat;
mod config;
mod connection;
mod methods;
mod state;

#[cfg(test)]
mod test;

/// Exposes the associated trackmania server globally.
static TRACKMANIA: OnceLock<TrackmaniaServer> = OnceLock::new();

/// Exposes the SpacetimeDB connection.
static SPACETIME: MyDbConnection = MyDbConnection::new();

/// Exposes the NadeoAPI with server auth.
static NADEO: OnceLock<Mutex<NadeoClient>> = OnceLock::new();

/// Path to the Filesystem of the trackmnia server UserData.
static TRACKMANIA_FILES: OnceLock<String> = OnceLock::new();

static SERVER_METADATA: OnceLock<Mutex<EventRawServerState>> = OnceLock::new();
//static EVENT_CACHE: LazyLock<StdMutex<VecDeque<Event>>> =
//LazyLock::new(|| StdMutex::new(VecDeque::with_capacity(1000)));

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt as _, util::SubscriberInitExt};

pub(crate) fn init_tracing_subscriber() {
    tracing_subscriber::registry()
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var("DEBUG_LOG_LEVEL")
                .from_env_lossy(),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Err(dbg_error) = dotenvy::from_path(env!("CARGO_MANIFEST_DIR").to_string() + "/.env")
        && let Err(prod_error) = dotenvy::dotenv()
    {
        tracing::warn!("No .env file was found. Error Prod: {prod_error}, Error Dbg: {dbg_error}")
    };

    init_tracing_subscriber();

    let tm_server_login = std::env::var("TM_MASTERSERVER_LOGIN")
        .expect("Environment variable: TM_MASTERSERVER_LOGIN MUST be set");
    let tm_server_password = std::env::var("TM_MASTERSERVER_PASSWORD")
        .expect("Environment variable: TM_MASTERSERVER_password MUST be set");
    std::env::var("TM_ACCOUNT_ID")
        .expect("Environment variable: TM_ACCOUNT_ID MUST be set at the moment.
        This will be the account where the server will be available under and can be obtained from e.g. trackmania.io. 
        We hope to make this optional in the future but depend on a change from nadeo on that sooo good luck ^^");
    let tm_server_url = std::env::var("TM_SERVER_URL").expect("Environment variable: TM_SERVER_URL MUST be set. This is needed to connect to the Trackmania server.");

    TRACKMANIA_FILES
        .set(std::env::var("TM_FILES").unwrap_or("./UserData".into()))
        .expect("The Path to the Trackmania Filesystem could not be established. Aborting.");

    let files = TRACKMANIA_FILES.wait();

    if !std::fs::exists(files).is_ok_and(|b| b) {
        panic!(
            "The TM_FILES variable is set to {} but the directory does not exist.
            Consider mounting the correct directory if you are using a docker container.",
            TRACKMANIA_FILES.wait()
        );
    } else {
        tracing::info!("Successfully detected the trackmania filesystem.");
    }

    if let Err(err) = std::fs::create_dir_all(files.clone() + "/Scripts/Modes/Trackmania") {
        tracing::error!(
            "Could not create the scripts directory. Perhaps you are missing permissions?. Reason: {err}"
        );
        panic!()
    }

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
        if let Ok(server) = TrackmaniaServer::new(tm_server_url.clone()).await {
            _ = TRACKMANIA.set(server);
        } else {
            let server = TrackmaniaServer::new(tm_server_url).await?;
            _ = TRACKMANIA.set(server);
        }
    }

    // Initial Configuration for the Trackmania server connection.
    {
        let server = TRACKMANIA.wait();

        let _: bool = server.call("SetApiVersion", "2023-03-25").await?;

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
    }

    if !SPACETIME.connect(false).await {
        tracing::error!("Could not connect to SpacetimeDB server");
        exit(1);
    };

    setup_state_synchronization().await;
    setup_chat().await;

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            tracing::error!("Unable to listen for shutdown signal: {}", err);
        }
    }

    Ok(())
}
