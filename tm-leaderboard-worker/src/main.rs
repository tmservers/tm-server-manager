use std::sync::OnceLock;

use nadeo_api_rs::{
    auth::{NadeoClient, UserAgentDetails},
    live::LiveApiClient,
};
use tm_tourney_manager_api_rs::{DbConnection, login_as_worker, post_record};
use tokio::signal;

static SPACETIME: OnceLock<DbConnection> = OnceLock::new();

static NADEO_API: OnceLock<NadeoClient> = OnceLock::new();

fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        //.on_connect_error(on_connect_error)
        //.on_disconnect(on_disconnected)
        .with_module_name(
            std::env::var("SPACETIMEDB_MODULE").unwrap_or("tm-tourney-manager".to_string()),
        )
        .with_uri(std::env::var("SPACETIMEDB_URL").unwrap_or("http://localhost:1234".to_string()))
        .build()
        .expect("Failed to connect to SpacetimeDB. Aborting.")
}

#[tokio::main]
async fn main() {
    dotenvy::from_path(env!("CARGO_MANIFEST_DIR").to_string() + "/.env")
        .expect(".env file couldn't be found!");

    let tm_server_login = std::env::var("TM_MASTERSERVER_LOGIN")
        .expect("Environment variable: TM_MASTERSERVER_LOGIN MUST be set");
    let tm_server_password = std::env::var("TM_MASTERSERVER_PASSWORD")
        .expect("Environment variable: TM_MASTERSERVER_PASSWORD MUST be set");
    let tm_account_id =
        std::env::var("TM_ACCOUNT_ID").expect("Environment variable: TM_ACCOUNT_ID MUST be set");

    let spacetime = connect_to_db();
    _ = SPACETIME.set(spacetime);
    tokio::spawn(async move {
        let spacetime = SPACETIME.wait();
        loop {
            _ = spacetime.run_async().await;
        }
    });

    //TODO check if the auth succeeded
    SPACETIME.wait().procedures.login_as_worker(
        tm_server_login.clone(),
        tm_server_password.clone(),
        tm_account_id.clone(),
    );

    let client = NadeoClient::create(
        nadeo_api_rs::auth::NadeoCredentials::DedicatedServer {
            u: tm_server_login,
            p: tm_server_password,
        },
        UserAgentDetails::new("tm-tourney-manager", "", "0"),
        2,
    )
    .await
    .unwrap();

    _ = NADEO_API.set(client);

    let client = NADEO_API.wait();

    let lb = client
        .get_map_leaderboard("vjyNNUu997cC5PW8e3x7Y9RsAF0", true, 100, 0)
        .await
        .unwrap()
        .tops
        .pop()
        .unwrap();
    for pos in lb.top.into_iter() {
        println!("{pos:?}");
        SPACETIME
            .wait()
            .reducers
            .post_record(
                "vjyNNUu997cC5PW8e3x7Y9RsAF0".into(),
                pos.accountId,
                pos.score as u32,
            )
            .unwrap();
    }

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
}
