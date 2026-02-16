use nadeo_api::{NadeoRequest, auth::AuthType, request::Method};
use serde::{Deserialize, Serialize};
use tm_server_controller::{ClientError, method::XmlRpcMethods};
use tm_tourney_manager_api_rs::{EventContext, ServerConfig};

use crate::{NADEO, SERVER_CONFIG, TRACKMANIA, state::sync};

pub fn config_update(_: &EventContext, new_config: &ServerConfig) {
    let new = new_config.clone();
    let Some(old_config) = SERVER_CONFIG.get() else {
        //The server has not been synced yet. Bootstrapping it.
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let server = TRACKMANIA.wait();
                _ = server
                    .chat_send_server_massage("[tm-server-bridge] Bootstrapping the server!")
                    .await;
                sync().await;
                configure(new).await;
                _ = server
                    .chat_send_server_massage("[tm-server-bridge] Bootstrapping successfull :>")
                    .await;
            });
        });
        return;
    };

    if old_config == new_config {
        return;
    }

    tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
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

    let written = local_server
        .write_file("MatchSettings/manager.txt", config.into())
        .await;

    //Configuration was successfully saved.
    if written.is_ok_and(|r| r) {
        // Load all maps to make them accessible locally
        get_maps(configuration.iter_maps()).await;
    }

    let loaded = local_server
        .load_match_settings("MatchSettings/manager.txt")
        .await;

    //TODO figure out what the returned integer means.
    //if loaded.is_ok_and(|l| l == 2) {
    if loaded.is_ok() {
        _ = local_server
            .chat_send_server_massage("[tm-server-bridge]   Server configuration synchronized.")
            .await;

        _ = local_server.next_map().await;
    }
}

pub(crate) async fn get_maps(maps: impl Iterator<Item = &String>) {
    //Only used in this function
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
        let _: Result<bool, ClientError> = TRACKMANIA
            .wait()
            .call(
                "ChatSendServerMessage",
                format!("[tm-server-bridge]   Imported map: {}$fff.", &map_info.name),
            )
            .await;
    }
}
