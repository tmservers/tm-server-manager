use std::{collections::VecDeque, sync::OnceLock, time::Duration};

use nadeo_api_rs::{
    auth::{NadeoClient, UserAgentDetails},
    live::LiveApiClient,
};
use spacetimedb_sdk::db_context::DbContext;
use spacetimedb_sdk::{Table, Uuid};
use tm_server_manager_api_rs::{DbConnection, MyJobsTableAccess, login_as_worker, post_record};
use tokio::{
    signal,
    sync::Mutex,
    task::spawn_blocking,
    time::{Instant, sleep_until},
};

static SPACETIME: OnceLock<DbConnection> = OnceLock::new();

static NADEO_API: OnceLock<NadeoClient> = OnceLock::new();

static JOB_QUEUE: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn connect_to_db() -> DbConnection {
    DbConnection::builder()
        //.on_connect_error(on_connect_error)
        //.on_disconnect(on_disconnected)
        //.on_connect(|c, i, w| println!("{i}"))
        .with_database_name(
            std::env::var("SPACETIMEDB_MODULE").unwrap_or("tm-server-manager".to_string()),
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
        UserAgentDetails::new("tm-server-manager", "", "0"),
        2,
    )
    .await
    .unwrap();

    _ = NADEO_API.set(client);

    let client = NADEO_API.wait();

    //TODO error handling.
    SPACETIME
        .wait()
        .subscription_builder()
        .subscribe("SELECT * FROM my_jobs");

    _ = JOB_QUEUE.set(Mutex::new(VecDeque::new()));

    //Keep Queue up to data.
    {
        SPACETIME.wait().db.my_jobs().on_insert(|_ctx, row| {
            let map = row.map_uid.clone();
            spawn_blocking(|| {
                let mut queue = JOB_QUEUE.wait().blocking_lock();
                println!("REACHED");
                queue.push_back(map);
            });
        });

        SPACETIME.wait().db.my_jobs().on_delete(|_ctx, row| {
            let map = row.map_uid.clone();
            spawn_blocking(move || {
                let mut queue = JOB_QUEUE.wait().blocking_lock();
                println!("REACHED");
                let map_pos = queue.iter().position(|v| *v == map);
                if let Some(position) = map_pos {
                    queue.remove(position);
                }
            });
        });
    }

    tokio::spawn(async {
        loop {
            sleep_until(Instant::now() + Duration::from_secs(60)).await;
            println!("LOOPED");

            let mut queue = JOB_QUEUE.wait().lock().await;

            let Some(map) = queue.front().cloned() else {
                println!("Queue was empty");
                continue;
            };
            queue.rotate_right(1);

            let lb = client
                .get_map_leaderboard(&map, true, 100, 0)
                .await
                .unwrap()
                .tops
                .pop()
                .unwrap();
            println!("{lb:?}");
            for pos in lb.top.into_iter() {
                let account_id = Uuid::parse_str(&pos.accountId).unwrap();
                println!("{pos:?}");
                SPACETIME
                    .wait()
                    .reducers
                    .post_record(map.clone(), account_id, pos.score as u32)
                    .unwrap();
            }
        }
    });

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
}
