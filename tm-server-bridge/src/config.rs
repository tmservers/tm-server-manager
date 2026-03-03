use nadeo_api::{NadeoRequest, auth::AuthType, request::Method};
use serde::{Deserialize, Serialize};
use tm_server_controller::method::XmlRpcMethods;
use tm_tourney_manager_api_rs::{EventContext, ServerConfig};

use crate::{NADEO, SERVER_CONFIG, TRACKMANIA, TRACKMANIA_FILES};

pub fn config_update(_: &EventContext, new_config: &ServerConfig) {
    let new = new_config.clone();

    let old_config = SERVER_CONFIG.get();

    if old_config.is_some() && old_config.unwrap() == new_config {
        return;
    }

    tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            let server = TRACKMANIA.wait();
            _ = server
                .chat_send_server_massage("[tmservers.live] New configuration is loading.")
                .await;
            configure(new).await;
        });
    });
}

pub async fn configure(server_config: ServerConfig) {
    let local_server = TRACKMANIA.wait();

    //SAFETY: Same type but rust can't know that.
    let configuration = unsafe {
        std::mem::transmute::<
            tm_tourney_manager_api_rs::ServerConfig,
            tm_server_controller::config::ServerConfig,
        >(server_config)
    };

    let config = configuration.into_xml();

    tracing::info!("Attempt to load configuration: {config}");

    let full_path = TRACKMANIA_FILES.wait().clone() + "/Maps/MatchSettings/manager.txt";

    if let Err(error) = std::fs::write(&full_path, config) {
        tracing::error!("Could not write the configuration file: {error}");
        return;
    }

    // Load all maps to make them accessible locally
    get_maps(configuration.iter_maps()).await;

    let loaded = local_server
        .load_match_settings("MatchSettings/manager.txt")
        .await;

    //TODO figure out what the returned integer means.
    //if loaded.is_ok_and(|l| l == 2) {
    if loaded.is_ok() {
        _ = local_server.next_map().await;

        _ = local_server
            .chat_send_server_massage("[tmservers.live] Configuration synchronized.")
            .await;

        tracing::info!("Loaded new configuration");
    } else {
        tracing::error!("There was an error loading the new configuration file.")
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
