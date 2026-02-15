use spacetimedb_sdk::Uuid;
use tm_server_controller::{
    callbacks::TypedCallbacks,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tm_tourney_manager_api_rs::{raw_server_player_add, raw_server_player_remove};

use crate::{SPACETIME, TRACKMANIA};

pub async fn setup_state_synchronization() {
    let server = TRACKMANIA.wait();

    server.on_player_connect(|player| {
        _ = SPACETIME.wait().reducers.raw_server_player_add(
            Uuid::parse_str(&player.account_id).unwrap(),
            player.is_spectator,
        )
    });

    server.on_player_disconnect(|player| {
        _ = SPACETIME
            .wait()
            .reducers
            .raw_server_player_remove(Uuid::parse_str(&player.account_id).unwrap())
    });

    server.on_player_info_changed(|player| {
        let spacetime = SPACETIME.wait();
        if player.spectator_status == 0 {
            _ = spacetime
                .reducers
                .raw_server_player_add(Uuid::parse_str(&player.account_id).unwrap(), false)
        } else {
            _ = spacetime
                .reducers
                .raw_server_player_add(Uuid::parse_str(&player.account_id).unwrap(), true)
        }
    });
}

/// Synchronizes all the state already present on the server with spacetime db.
pub async fn sync() {
    let local_server = TRACKMANIA.wait();
    let spacetime = SPACETIME.wait();
    if let Ok(players) = local_server.get_player_list().await {
        for player in players {
            //TODO investigate spectator status return again.
            if player.spectator_status == 0 {
                _ = spacetime
                    .reducers
                    .raw_server_player_add(Uuid::parse_str(&player.account_id).unwrap(), false);
            } else {
                _ = spacetime
                    .reducers
                    .raw_server_player_add(Uuid::parse_str(&player.account_id).unwrap(), true);
            }
        }
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
