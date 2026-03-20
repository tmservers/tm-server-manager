use nadeo_api::{NadeoRequest, auth::AuthType, request::Method};
use serde::{Deserialize, Serialize};
use tm_server_controller::method::XmlRpcMethods;
use tm_server_manager_api_rs::EventContext;
use tm_server_types::config::ServerConfig;
use tokio::sync::Mutex;

use crate::{NADEO, SERVER_CONFIG, TRACKMANIA, TRACKMANIA_FILES};

pub fn config_update(_: &EventContext, new_config: &tm_server_manager_api_rs::ServerConfig) {
    //SAFETY: Same type but rust can't know that.
    let configuration = unsafe {
        std::mem::transmute::<
            tm_server_manager_api_rs::ServerConfig,
            tm_server_controller::config::ServerConfig,
        >(new_config.clone())
    };

    tokio::task::block_in_place(move || {
        let old_config = SERVER_CONFIG.get();
        tokio::runtime::Handle::current().block_on(async move {
            {
                if old_config.is_some() && *old_config.unwrap().lock().await == configuration {
                    return;
                }
            }

            let server = TRACKMANIA.wait();

            _ = server
                .chat_send_server_massage("[tmservers.live] New configuration is loading.")
                .await;
            configure(configuration).await;

            _ = server
                .chat_send_server_massage("[tmservers.live] New configuration loaded.")
                .await;
        });
    });
}

pub async fn configure(server_config: ServerConfig) {
    let local_server = TRACKMANIA.wait();

    let config = server_config.into_xml();

    tracing::info!("Attempt to load configuration: {config}");

    let full_path = TRACKMANIA_FILES.wait().clone() + "/Maps/MatchSettings/manager.txt";

    if let Err(error) = std::fs::write(&full_path, config) {
        tracing::error!("Could not write the configuration file: {error}");
        return;
    }

    // Load all maps to make them accessible locally
    get_maps(server_config.iter_maps()).await;

    let loaded = local_server
        .load_match_settings("MatchSettings/manager.txt")
        .await;

    // The i32 is the map count which is not important to verify.
    if loaded.is_ok() {
        {
            let mut locked = SERVER_CONFIG
                .get_or_init(|| Mutex::new(server_config.clone()))
                .lock()
                .await;
            *locked = server_config;
        }

        _ = local_server.restart_map().await;
        //TODO remove.
        //let _: Result<(), ClientError> = local_server.call("GetModeScriptSettings", ()).await;

        tracing::info!("Loaded new configuration");
    } else {
        tracing::error!("There was an error loading the new configuration file. {loaded:?}")
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
