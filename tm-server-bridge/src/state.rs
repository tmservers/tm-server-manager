use spacetimedb_sdk::Uuid;
use tm_server_controller::{
    callbacks::TypedCallbacks,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tm_server_types::{
    base::PlayerInfo,
    event::{EndRoundStart, PlayerConnect, PlayerDisconnect},
};
use tm_tourney_manager_api_rs::{
    post_event, post_round_replay, raw_server_player_add, raw_server_player_remove,
};

use crate::{SPACETIME, TRACKMANIA, TRACKMANIA_FILES};

pub async fn setup_state_synchronization() {
    let server = TRACKMANIA.wait();

    // Sync all events to spacetimedb.
    server.on_event(|event| {
        let spacetime = SPACETIME.wait();
        if spacetime
            .reducers
            .post_event(
                //SAFETY: Its the same type. Sadly Rust can not know that :< .
                unsafe {
                    std::mem::transmute::<
                        tm_server_controller::event::Event,
                        tm_tourney_manager_api_rs::Event,
                    >(event.clone())
                },
            )
            .is_err()
        {
            println!("Event failed to publish!")
        }
    });

    server.on_end_round_start(async |event: &EndRoundStart| {
        let file_name = format!("{}{}", event.count, event.time);
        if let Err(error) = server.save_current_replay(&file_name).await {
            tracing::error!("Failed to save Replay File after Round ended. Reason: {error}");
            return;
        };
        let full_path = TRACKMANIA_FILES.wait().clone()
            + "/Replays/"
            + &std::env::var("TM_MASTERSERVER_LOGIN").unwrap()
            + "/Autosaves/"
            + &file_name
            + ".Replay.Gbx";

        match std::fs::read(&full_path) {
            Ok(file) => {
                //TODO enable the posting again.
                //SPACETIME.wait().procedures.post_round_replay(file);
            }
            Err(error) => {
                tracing::error!("Failed to read replay file. Reason: {error}")
            }
        };
        if let Err(error) = std::fs::remove_file(&full_path) {
            tracing::error!("Failed to delete the current replay file! Reason: {error}")
        };
    })
}

/// Synchronizes all the state already present on the server with spacetime db.
pub async fn sync() {
    let local_server = TRACKMANIA.wait();
    let spacetime = SPACETIME.wait();
    if let Ok(players) = local_server.get_player_list().await {
        tracing::error!("{players:?}");
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
