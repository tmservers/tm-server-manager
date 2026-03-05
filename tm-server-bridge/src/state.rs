use spacetimedb_sdk::Uuid;
use tm_server_controller::{
    ClientError,
    callbacks::TypedCallbacks,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tm_server_types::event::{EndRoundStart, StartMatch, StartServer};
use tm_tourney_manager_api_rs::{post_event, raw_server_player_add};

use crate::{SERVER_CONFIG, SPACETIME, TRACKMANIA, TRACKMANIA_FILES};

pub async fn setup_state_synchronization() {
    let server = TRACKMANIA.wait();

    sync_players().await;

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
            tracing::error!("Event failed to publish!")
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
            Ok(_file) => {
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
    });

    server.on_start_server_start(async |event: &StartServer| {
        if event.mode.updated {
            tracing::error!("Mode Script was updated");
            //We need to load the settings again because we changed the script.
            if let Err(error) = TRACKMANIA
                .wait()
                .set_mode_script_settings(SERVER_CONFIG.wait().lock().await.clone())
                .await
            {
                tracing::error!("{error}")
            };
            //TODO remove.
            let _: Result<(), ClientError> =
                TRACKMANIA.wait().call("GetModeScriptSettings", ()).await;
        } else {
            //We should be fine because the settings already loaded correctly.
            tracing::error!("Mode Script stayed the same");
        }
    });

    server.on_start_match_start(async |_: &StartMatch| {
        //We need to load the settings again because we changed the script.
        if let Err(error) = TRACKMANIA
            .wait()
            .set_mode_script_settings(SERVER_CONFIG.wait().lock().await.clone())
            .await
        {
            tracing::error!("{error}")
        };
        //TODO remove.
        let _: Result<(), ClientError> = TRACKMANIA.wait().call("GetModeScriptSettings", ()).await;
    });
}

/// Synchronizes all the state already present on the server with spacetime db.
pub(super) async fn sync_players() {
    let server = TRACKMANIA.wait();
    let spacetime = SPACETIME.wait();
    if let Ok(players) = server.get_player_list().await {
        for player in players {
            // This is the server itself so skip the sync.
            if player.flags & 0b100000 != 0 {
                continue;
            }

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
}
