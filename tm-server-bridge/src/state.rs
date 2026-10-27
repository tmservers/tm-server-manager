use std::time::Duration;

use spacetimedb_sdk::{Table, Uuid};
use tm_server_controller::{
    callbacks::TypedCallbacks,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tm_server_manager_api_rs::{
    RawServerPermittedPlayersTableAccess, RawServerPlayerDestinationTableAccess, post_event,
    post_round_replay, raw_server_player_add,
};
use tm_server_types::{
    base::account_id_to_login,
    event::{EndRoundStart, PlayerConnect, StartMap, StartServer},
};
use tokio::time::sleep;

use crate::{SERVER_METADATA, SPACETIME, TRACKMANIA, TRACKMANIA_FILES};

pub async fn setup_state_synchronization() {
    let server = TRACKMANIA.get().unwrap();

    // Sync all events to spacetimedb.
    server.on_event(|event| {
        let spacetime = SPACETIME.read();
        if spacetime
            .reducers
            .post_event(
                //SAFETY: Its the same type. Sadly Rust can not know that :< .
                unsafe {
                    std::mem::transmute::<
                        tm_server_controller::event::Event,
                        tm_server_manager_api_rs::Event,
                    >(event.clone())
                },
            )
            .is_err()
        {
            tracing::error!("Event failed to publish!")
        }
    });

    // Sync the replay of every round to the server.
    server.on_end_round_start(async |event: &EndRoundStart| {
        tracing::info!("Trying to save replay of this round.");
        let file_name = format!("{}{}", event.count, event.time);
        match server.save_current_replay(&file_name).await {
            Ok(b) if !b => {
                tracing::error!("Did not save successfully.");
                return;
            }
            Err(err) => {
                tracing::error!("Failed to save Replay File after Round ended. Reason: {err}");
                return;
            }
            _ => (),
        };

        let full_path = TRACKMANIA_FILES.wait().clone()
            + "/Replays/"
            + &std::env::var("TM_MASTERSERVER_LOGIN").unwrap()
            + "/Autosaves/"
            + &file_name
            + ".Replay.Gbx";

        let mut seconds = 10;
        while seconds > 0 {
            match std::fs::read(&full_path) {
                Ok(file) => {
                    SPACETIME.read().procedures.post_round_replay_then(
                        event.time,
                        file,
                        |_, res| {
                            tracing::info!("Posting replay returned: {res:?}");
                        },
                    );
                    if let Err(error) = std::fs::remove_file(&full_path) {
                        tracing::error!("Failed to delete the current replay file! Reason: {error}")
                    };
                    return;
                }
                Err(error) => {
                    tracing::error!("Failed to read replay file {full_path}. Reason: {error}")
                }
            };

            seconds -= 2;
            sleep(Duration::from_secs(2)).await;
        }
    });

    server.on_player_connect(async |event: &PlayerConnect| {
        // Player destination
        move_player_to_destination(Uuid::parse_str(&event.account_id).unwrap()).await;

        let server = TRACKMANIA.get().unwrap();

        // Server allowlist.
        if let Some(meta) = SERVER_METADATA.get()
            && meta.lock().await.open
        {
            tracing::info!("Server is open skipping check if player is permitted.");
            return;
        } else {
            tracing::info!("Not having SERVER_METADATA available yet. Assuming server is closed.");
        }
        let Some(player) = SPACETIME
            .read()
            .db
            .raw_server_permitted_players()
            .iter()
            .find(|p| Uuid::parse_str(&event.account_id).unwrap() == p.account_id)
        else {
            tracing::warn!("Player tried to connect without the required permissions.");
            if let Err(error) = server
                .kick(
                    event.account_id.clone(),
                    "Not allowed to participate in the server.",
                )
                .await
            {
                tracing::error!("Could not kick player: {error}")
            };

            return;
        };
        if player.only_spectator {
            tracing::warn!(
                "Forcing player to spectator: {}",
                player.account_id.to_uuid()
            );
            if let Err(err) = TRACKMANIA
                .wait()
                .force_spectator(player.account_id.to_string(), 1)
                .await
            {
                tracing::error!("Could not force player to spectator. Error {err}");
            }
        }
    });

    server.on_start_server_end(async |event: &StartServer| {
        if event.mode.updated {
            tracing::info!("Mode Script was updated");
        } else {
            tracing::info!("Mode Script stayed the same");
        }
    });

    server.on_start_map_start(async |start: &StartMap| {
        tracing::info!("Starting new map");

        if start.restarted
            && let Some(lock) = SERVER_METADATA.get()
        {
            tracing::info!("Reapplying config on StartMapStart.");
            let config = unsafe {
                std::mem::transmute::<
                    tm_server_manager_api_rs::ServerConfigV2,
                    tm_server_controller::config::ServerConfigV2,
                >(lock.lock().await.config.clone())
            };

            if let Err(error) = TRACKMANIA.wait().set_mode_script_settings(config).await {
                tracing::error!("{error}")
            };
        }
    });

    server.on_start_map_end(async |start: &StartMap| {
        tracing::info!("Ending to start new map");
        if start.restarted
            && let Some(lock) = SERVER_METADATA.get()
        {
            tracing::info!("Reapplying config on StartMapEnd.");
            let config = unsafe {
                std::mem::transmute::<
                    tm_server_manager_api_rs::ServerConfigV2,
                    tm_server_controller::config::ServerConfigV2,
                >(lock.lock().await.config.clone())
            };

            if let Err(error) = TRACKMANIA.wait().set_mode_script_settings(config).await {
                tracing::error!("{error}")
            };
        }
    });
}

/// Synchronizes all the state already present on the server with spacetime db.
pub(super) async fn sync_players() {
    let server = TRACKMANIA.get().unwrap();
    let spacetime = SPACETIME.read();
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

pub fn check_allowed_players() {
    tracing::info!(
        "Checking allowed players... have new player list (account_id, only_spectator): {:?}",
        SPACETIME
            .read()
            .db
            .raw_server_permitted_players()
            .iter()
            .map(|p| (p.account_id.to_string(), p.only_spectator))
            .collect::<Vec<_>>()
    );
    tokio::spawn(async {
        if let Some(meta) = SERVER_METADATA.get()
            && meta.lock().await.open
        {
            tracing::info!("Server is open skipping check of permitted playeres.");
            return;
        } else {
            tracing::info!(
                "Not having SERVER_METADATA available yet. Assuming server is not open."
            );
        }
        let server = TRACKMANIA.get().unwrap();
        tracing::info!("Server not open proceeding with kick...");
        if let Ok(players) = server.get_player_list().await {
            tracing::info!("Current Players on the server for allwoed player check: {players:?}");
            for server_player in players {
                let Some(player) = SPACETIME
                    .read()
                    .db
                    .raw_server_permitted_players()
                    .iter()
                    .find(|p| Uuid::parse_str(&server_player.account_id).unwrap() == p.account_id)
                else {
                    // This is the server itself so skip
                    if server_player.flags & 0b100000 != 0 {
                        continue;
                    }
                    tracing::info!(
                        "Kicking player on the server which is not allowed anymore: {}",
                        server_player.account_id
                    );
                    if let Err(error) = server
                        .kick(
                            server_player.account_id.clone(),
                            "Not allowed to be on the server.",
                        )
                        .await
                    {
                        tracing::error!("Could not kick player: {error}")
                    };

                    continue;
                };
                if player.only_spectator {
                    tracing::warn!(
                        "Forcing player as spectator. {}",
                        player.account_id.to_string()
                    );
                    if let Err(err) = server
                        .force_spectator(player.account_id.to_string(), 1)
                        .await
                    {
                        tracing::error!("Could not force player to spectator. Error {err}");
                    }
                } else {
                    tracing::warn!(
                        "Releasing player from spectator only. {}",
                        player.account_id.to_string()
                    );
                    if let Err(err) = server
                        .force_spectator(player.account_id.to_string(), 0)
                        .await
                    {
                        tracing::error!("Could not force player to spectator. Error {err}");
                    }
                }
            }
        } else {
            tracing::error!("Could not receive players list for kicking.");
        }
    });
}

pub fn check_players_have_destination() {
    tokio::spawn(async move {
        let server = TRACKMANIA.wait();
        if let Ok(players) = server.get_player_list().await {
            for server_player in players {
                move_player_to_destination(Uuid::parse_str(&server_player.account_id).unwrap())
                    .await;
            }
        }
    });
}

pub async fn seamless_recovery() {
    let server = TRACKMANIA.wait();
    let mut seconds = 20;
    while seconds > 0 {
        if let Err(err) = server
            .chat_send_server_massage(format!(
                "[tmservers.live] Match will resume in {seconds} seconds."
            ))
            .await
        {
            tracing::error!("Error sending resume message. Reason: {}", err)
        };
        seconds -= 2;
        sleep(Duration::from_secs(2)).await;
    }

    if let Err(err) = server
        .chat_send_server_massage("[tmservers.live] Match resuming. GLHF.")
        .await
    {
        tracing::error!("Error sending resume message. Reason: {}", err)
    };
    if let Err(err) = server.pause_set_active(false).await {
        tracing::error!(
            "Error unpausing the match because spacetime reconnected. Reson: {}",
            err
        )
    }
}

async fn move_player_to_destination(account_id: Uuid) {
    let server = TRACKMANIA.wait();

    if let Some(player) = SPACETIME
        .read()
        .db
        .raw_server_player_destination()
        .iter()
        .find(|p| account_id == p.account_id)
    {
        /* let mut seconds = 10;
        while seconds > 0 {
            if let Err(err) = server
                .chat_send_to_account(format!(
                    "[tmservers.live] Found a destination for you you will be automatically moved to a new server in {seconds} seconds."
                ),vec![account_id.to_string()])
                .await
            {
                tracing::error!("Error sending resume message. Reason: {}", err)
            };
            seconds -= 2;
            sleep(Duration::from_secs(2)).await;
        } */

        if let Err(error) = server
            .send_open_link_to_account(
                player.account_id.to_string(),
                format!(
                    "#qjoin={}@Trackmania",
                    account_id_to_login(&player.server_account_id.to_string())
                ),
                1,
            )
            .await
        {
            tracing::error!("Could not send link: {error}")
        };
    };
}
