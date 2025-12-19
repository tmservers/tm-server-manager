use tm_server_controller::method::{ModeScriptMethodsXmlRpc, XmlRpcMethods};
use tm_tourney_manager_api_rs::ServerState;
use tracing::{info, warn};

use crate::{SPACETIME, TRACKMANIA};

/// Synchronizes all the state already present on the server with spacetime db.
pub async fn sync() {
    let local_server = TRACKMANIA.wait();
    let spacetime = SPACETIME.wait();
    if let Ok(players) = local_server.get_player_list().await {
        for player in players {
            if player.SpectatorStatus == 0 {}
        }
        //spacetime.reducers.set_tm_server_state()
    } else {
        tracing::error!(
            "Failed to fetch the player list and thus could not syncronize server state! Aborting.."
        );
        std::process::exit(1)
    }
    _ = local_server
        .chat_send_server_massage("[tm-server-bridge]   Server state synchronized.")
        .await;
}
