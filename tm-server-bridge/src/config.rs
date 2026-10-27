use std::time::Duration;

use tokio::{sync::Mutex, time::sleep};

use nadeo_api::{NadeoRequest, auth::AuthType, request::Method};
use serde::{Deserialize, Serialize};
use tm_server_controller::method::XmlRpcMethods;
use tm_server_manager_api_rs::{EventContext, EventRawServerState};

use crate::{NADEO, SERVER_METADATA, TRACKMANIA, TRACKMANIA_FILES, state::check_allowed_players};

pub fn metadata_update(_: &EventContext, new_metadata: &EventRawServerState) {
    tracing::info!("Received new Server metadata. Trying to apply...");
    let new_metadata = new_metadata.clone();
    tokio::spawn(async move {
        let config = unsafe {
            std::mem::transmute::<
                tm_server_manager_api_rs::ServerConfigV2,
                tm_server_controller::config::ServerConfigV2,
            >(new_metadata.config.clone())
        };

        // Get the script if its a non built in mode.
        if let Some(script) = config.get_mode().get_external_script() {
            let full_path = TRACKMANIA_FILES.wait().clone()
                + "/Scripts/"
                + config.get_mode().script_name()
                + ".Script.txt";

            if let Err(error) = std::fs::write(&full_path, script) {
                tracing::error!("Could not write the mode script file: {error}");
            }
        }

        tracing::info!("New configuration is loading.");

        get_maps(config.iter_maps()).await;
        let config = config.into_xml();

        tracing::info!("New saved config is: {config}");

        let full_path = TRACKMANIA_FILES.wait().clone() + "/Maps/MatchSettings/manager.txt";

        if let Err(error) = std::fs::write(&full_path, config) {
            tracing::error!("Could not write the configuration file: {error}");
        }

        load_new_config(&new_metadata).await;

        // We wait before checking to ensure the new allowlist is there.
        // This is not the cleanest solution but is fine for now.
        sleep(Duration::from_secs(2)).await;
        check_allowed_players();
    });
}

async fn load_new_config(new_metadata: &EventRawServerState) {
    let server = TRACKMANIA.get().unwrap();

    let mut loaded = server
        .load_match_settings("MatchSettings/manager.txt")
        .await;

    while let Err(err) = loaded {
        tracing::error!("Could not load match config. Reason: {}", err);
        sleep(Duration::from_secs(2)).await;
        loaded = server
            .load_match_settings("MatchSettings/manager.txt")
            .await;
    }

    let mut restarted = server.next_map().await;

    while let Err(err) = restarted {
        tracing::error!("Could not restart!. Reason: {err}");
        sleep(Duration::from_secs(2)).await;
        restarted = server.next_map().await;
    }

    if loaded.is_ok() {
        let mut locked = SERVER_METADATA
            .get_or_init(|| Mutex::new(new_metadata.clone()))
            .lock()
            .await;
        *locked = new_metadata.clone();
    } else {
        tracing::error!("Could not load new config.")
    }

    if let Err(err) = server
        .chat_send_server_massage("Applied new configuration.")
        .await
    {
        tracing::error!("Could not send server message!. Reason: {err}");
    };

    tracing::info!("Applied new configuration.");
}

async fn get_maps(maps: impl Iterator<Item = &String>) {
    #[derive(Debug, Serialize, Deserialize)]
    struct MapInfo {
        #[serde(rename = "fileUrl")]
        file_url: String,
        #[serde(rename = "mapUid")]
        map_uid: String,
        name: String,
    }

    //TODO: better to use the mapUidList and afterwards make a for loop  to reduce nadeo api calls.
    for map in maps {
        let req = NadeoRequest::builder()
            .method(Method::GET)
            .auth_type(AuthType::NadeoServices)
            .url(&format!(
                "https://prod.trackmania.core.nadeo.online/maps/?mapUidList={map}"
            ))
            .build()
            .unwrap();
        let resp = NADEO.wait().lock().await.execute(req).await;

        let map_info: Vec<MapInfo> = resp.unwrap().json().await.unwrap();
        let map_info = &map_info[0];

        let req = NadeoRequest::builder()
            .method(Method::GET)
            .auth_type(AuthType::NadeoServices)
            .url(&map_info.file_url)
            .build()
            .unwrap();

        let resp = NADEO.wait().lock().await.execute(req).await;
        let map_file = resp.unwrap().bytes().await.unwrap();
        _ = TRACKMANIA
            .wait()
            .write_file(&format!("{}.Map.Gbx", &map_info.map_uid), map_file.to_vec())
            .await;
    }
}

//SetModeScriptText(string)
