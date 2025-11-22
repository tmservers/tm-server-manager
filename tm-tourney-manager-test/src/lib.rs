#![allow(dead_code, unused_imports)]
use std::sync::OnceLock;

use spacetimedb_sdk::db_context::DbContext;
use testcontainers::{Image, core::ContainerState, runners::SyncRunner};
use tm_tourney_manager_api_rs::{DbConnection, TournamentTableAccess, create_tournament};
use tokio::runtime;

const PROJECT_NAME: &str = "tm-tourney-manager";

#[test]
#[should_panic]
fn enforce_unique_tournament_name() {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    };

    let dir = env!("CARGO_MANIFEST_DIR").rsplit_once(r#"\"#).unwrap().0;

    let path = "file://".to_string() + dir + r#"\"# + PROJECT_NAME;

    _ = std::process::Command::new("cargo")
        .args([
            "build",
            "--package",
            &path,
            "--target",
            "wasm32-unknown-unknown",
        ])
        .output()
        .expect("failed to build your spacetime server module.");

    let spacetime = SyncRunner::start(
        GenericImage::new("clockworklabs/spacetime", "v1.9.0")
            .with_exposed_port(3000.tcp())
            .with_wait_for(WaitFor::message_on_stdout(
                "spacetimedb-standalone version: 1.9.0",
            ))
            .with_cmd(["start"]),
    )
    .unwrap();

    let spacetime_url = format!(
        "http://{}:{}",
        spacetime.get_host().unwrap(),
        spacetime.get_host_port_ipv4(3000).unwrap()
    );

    let published_module_name: String = "howdoideterminethisname".into();

    let published_dir = dir.to_string()
        + "/target/wasm32-unknown-unknown/debug/"
        + &PROJECT_NAME.replace("-", "_")
        + ".wasm";

    _ = std::process::Command::new("spacetime")
        .args([
            "publish",
            "-b",
            &published_dir,
            "-s",
            &spacetime_url,
            &published_module_name,
        ])
        .output()
        .expect("failed to build your spacetime server module.");

    let con = DbConnection::builder()
        .with_module_name(published_module_name)
        .with_uri(spacetime_url)
        .build()
        .expect("Failed to connect to SpacetimeDB. Aborting.");

    static SPACETIME: OnceLock<DbConnection> = OnceLock::new();

    _ = SPACETIME.set(con);

    let spacetime = SPACETIME.wait();

    spacetime.reducers.on_create_tournament(|t, _| {
        if let spacetimedb_sdk::Status::Failed(event) = &t.event.status {
            panic!("{event}")
        }
    });

    _ = spacetime
        .reducers
        .create_tournament("this is a banger tournament".into());

    _ = spacetime
        .reducers
        .create_tournament("this is a banger tournament".into());

    loop {
        SPACETIME.wait().advance_one_message().unwrap();
    }
}
