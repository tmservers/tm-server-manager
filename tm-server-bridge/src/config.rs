use nadeo_api::{NadeoRequest, auth::AuthType, request::Method};
use serde::{Deserialize, Serialize};
use tm_server_controller::method::XmlRpcMethods;
use tm_server_manager_api_rs::{EventContext, ServerMetadata};
use tokio::sync::Mutex;

use crate::{NADEO, SERVER_METADATA, TRACKMANIA, TRACKMANIA_FILES};

pub fn metadata_update(_: &EventContext, new_metadata: &tm_server_manager_api_rs::ServerMetadata) {
    tokio::task::block_in_place(move || {
        let old_metadata = SERVER_METADATA.get();
        tokio::runtime::Handle::current().block_on(async move {
            let server = TRACKMANIA.wait();

            _ = server
                .chat_send_server_massage("[tmservers.live] New configuration is loading.")
                .await;
            if old_metadata.is_some()
                && old_metadata.unwrap().lock().await.config == new_metadata.config
            {
                _ = server.restart_map().await;
                _ = server
                    .chat_send_server_massage(
                        "[tmservers.live] Configuration stayed the same restarting regardless.",
                    )
                    .await;
                return;
            }
            if configure(new_metadata.clone()).await {
                _ = server.restart_map().await;
            }

            _ = server
                .chat_send_server_massage("[tmservers.live] New configuration loaded.")
                .await;
        });
    });
}

pub async fn configure(server_metadata: ServerMetadata) -> bool {
    let local_server = TRACKMANIA.wait();

    let server_config = unsafe {
        std::mem::transmute::<
            tm_server_manager_api_rs::ServerConfig,
            tm_server_controller::config::ServerConfig,
        >(server_metadata.config.clone())
    };

    let config = server_config.into_xml();

    tracing::info!("Attempt to load configuration: {config}");

    let full_path = TRACKMANIA_FILES.wait().clone() + "/Maps/MatchSettings/manager.txt";

    if let Err(error) = std::fs::write(&full_path, config) {
        tracing::error!("Could not write the configuration file: {error}");
        return false;
    }

    // Load all maps to make them accessible locally
    get_maps(server_config.iter_maps()).await;

    let loaded = local_server
        .load_match_settings("MatchSettings/manager.txt")
        .await;

    // The i32 is the map count which is not important to verify.
    if loaded.is_ok() {
        {
            let mut locked = SERVER_METADATA
                .get_or_init(|| Mutex::new(server_metadata.clone()))
                .lock()
                .await;
            *locked = server_metadata;
        }

        //TODO remove.
        //let _: Result<(), ClientError> = local_server.call("GetModeScriptSettings", ()).await;

        tracing::info!("Loaded new configuration");
        true
    } else {
        tracing::error!("There was an error loading the new configuration file. {loaded:?}");
        false
    }
}

pub(crate) async fn get_maps(maps: impl Iterator<Item = &String>) {
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
