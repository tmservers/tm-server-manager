pub mod callbacks;
mod core;
pub mod method;

pub use core::ClientError;
pub use core::TrackmaniaServer;

pub use tm_server_types::*;

#[tokio::test]
async fn server_connect_and_authenticate() -> Result<(), ClientError> {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    };

    let tm_user = std::env::var("TM_MASTERSERVER_LOGIN")
        .expect("Environment variable: TM_MASTERSERVER_LOGIN MUST be set");
    let tm_password = std::env::var("TM_MASTERSERVER_PASSWORD")
        .expect("Environment variable: TM_MASTERSERVER_password MUST be set");

    let container = GenericImage::new("evoesports/trackmania", "latest")
        .with_exposed_port(2350.tcp())
        .with_exposed_port(2350.udp())
        .with_exposed_port(5000.tcp())
        .with_wait_for(WaitFor::message_on_stdout("...Load succeeds"))
        .with_env_var("TM_MASTERSERVER_LOGIN", tm_user)
        .with_env_var("TM_MASTERSERVER_PASSWORD", tm_password)
        .with_env_var("TM_SYSTEM_XMLRPC_ALLOWREMOTE", "True")
        .start()
        .await
        .unwrap();

    println!("{container:?}");

    let tm_url = format!(
        "{}:{}",
        container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5000).await.unwrap()
    );

    println!("{:?}", tm_url);

    let server = TrackmaniaServer::new(tm_url).await?;
    let success: Result<bool, ClientError> = server.call("SetApiVersion", "2025-07-04").await;

    println!("{:?}", success);
    assert!(success.is_ok() && success.unwrap());
    let success: Result<bool, ClientError> = server
        .call("Authenticate", ("SuperAdmin", "SuperAdmin"))
        .await;
    println!("{:?}", success);
    assert!(success.is_ok() && success.unwrap());

    Ok(())
}
