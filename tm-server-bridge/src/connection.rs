use std::sync::OnceLock;
use std::time::Duration;

use spacetimedb_sdk::DbContext;
use spacetimedb_sdk::EventTable;
use spacetimedb_sdk::Table;
use spacetimedb_sdk::Uuid;
use tm_server_controller::method::ModeScriptMethodsXmlRpc;
use tm_server_controller::method::XmlRpcMethods;
use tm_server_manager_api_rs::DbConnection;
use tm_server_manager_api_rs::ErrorContext;
use tm_server_manager_api_rs::EventRawServerMethodTableAccess;
use tm_server_manager_api_rs::EventRawServerStateTableAccess;
use tm_server_manager_api_rs::RawServerPermittedPlayersTableAccess;
use tm_server_manager_api_rs::RawServerPlayerDestinationTableAccess;
use tm_server_manager_api_rs::event_raw_server_methodQueryTableAccess;
use tm_server_manager_api_rs::event_raw_server_stateQueryTableAccess;
use tm_server_manager_api_rs::login_as_server;
use tm_server_manager_api_rs::raw_server_permitted_playersQueryTableAccess;
use tm_server_manager_api_rs::raw_server_player_destinationQueryTableAccess;
use tokio::sync::{RwLock, oneshot};
use tokio::time::sleep;

use crate::SPACETIME;
use crate::TRACKMANIA;
use crate::config::metadata_update;
use crate::methods::method_call_received;
use crate::state::check_allowed_players;
use crate::state::check_players_have_destination;
use crate::state::seamless_recovery;
use crate::state::sync_players;

pub struct MyDbConnection {
    db: OnceLock<RwLock<DbConnection>>,
}

impl MyDbConnection {
    pub const fn new() -> Self {
        MyDbConnection {
            db: OnceLock::new(),
        }
    }

    pub fn read(&self) -> tokio::sync::RwLockReadGuard<'_, DbConnection> {
        tokio::task::block_in_place(move || self.db.wait().blocking_read())
    }

    pub async fn connect(&self, seamless: bool) -> bool {
        let Ok(spacetime) = DbConnection::builder()
            .on_connect_error(on_connection_error)
            .on_disconnect(on_stdb_disconnected)
            .with_database_name(
                std::env::var("SPACETIMEDB_MODULE").unwrap_or("tmservers".to_string()),
            )
            .with_uri(
                std::env::var("SPACETIMEDB_URL")
                    .unwrap_or("wss://maincloud.spacetimedb.com".to_string()),
            )
            .build()
        else {
            tracing::error!("Server could not be connected successfully");
            return false;
        };

        let tm_server_login = std::env::var("TM_MASTERSERVER_LOGIN").unwrap();
        let tm_server_password = std::env::var("TM_MASTERSERVER_PASSWORD").unwrap();
        let tm_account_id = std::env::var("TM_ACCOUNT_ID").unwrap();
        let tm_account_id = Uuid::parse_str(&tm_account_id).unwrap();

        if let Some(val) = self.db.get() {
            let mut waited = val.write().await;
            *waited = spacetime;
        } else {
            _ = self.db.set(RwLock::new(spacetime));
        }

        tokio::spawn(async move {
            loop {
                let connection = SPACETIME.read();
                if let Err(err) = connection.advance_one_message_async().await {
                    tracing::error!(
                        "Spacetime stopped the run_async function unexpectedly. Reason: {err}"
                    );
                    break;
                };
            }
        });

        let spacetime = self.read();

        // Not every codegen emits the async procedure wrappers, the callback version always exists.
        let (login_sender, login) = oneshot::channel();
        spacetime.procedures.login_as_server_then(
            tm_server_login,
            tm_server_password,
            tm_account_id,
            seamless,
            move |_, result| {
                _ = login_sender.send(result);
            },
        );
        let server_id = match login.await {
            Ok(Ok(Ok(server_id))) => server_id,
            result => {
                tracing::error!("Server could not be authenticated successfully: {result:?}");
                return false;
            }
        };

        tracing::info!("Successfully connected to tmservers.live!");

        // Initialize state subscriptions for the server.
        _ = spacetime
            .subscription_builder()
            .on_applied(|_| tracing::debug!("Subscription successfully applied!"))
            .on_error(|_, error| tracing::error!("Subscription failed: {error:?}"))
            .add_query(|ctx| {
                ctx.from
                    .event_raw_server_method()
                    .r#where(|s| s.server_id.eq(server_id))
            })
            .add_query(|ctx| {
                ctx.from
                    .event_raw_server_state()
                    .r#where(|s| s.server_id.eq(server_id))
            })
            .add_query(|ctx| ctx.from.raw_server_permitted_players())
            .add_query(|ctx| ctx.from.raw_server_player_destination())
            .subscribe();

        spacetime
            .db
            .event_raw_server_state()
            .on_insert(metadata_update);

        spacetime
            .db
            .raw_server_permitted_players()
            .on_insert(|_, _| check_allowed_players());
        spacetime
            .db
            .raw_server_permitted_players()
            .on_delete(|_, _| check_allowed_players());

        spacetime
            .db
            .event_raw_server_method()
            .on_insert(method_call_received);

        spacetime
            .db
            .raw_server_player_destination()
            .on_insert(|_, _| check_players_have_destination());

        sync_players().await;

        true
    }
}

fn on_connection_error(ctx: &ErrorContext, error: spacetimedb_sdk::Error) {
    tracing::error!("{:?}", ctx.event);
    tracing::error!("{error:?}");
    tokio::spawn(spacetime_disconnected());
}

fn on_stdb_disconnected(_: &ErrorContext, err: Option<spacetimedb_sdk::Error>) {
    if let Some(err) = err {
        tracing::error!(
            "Forcefully disconnected from SpacetimeDB with Error: {}",
            err
        );
    }
    tracing::error!("Disconnected from spacetimedb.");
    tokio::spawn(spacetime_disconnected());
}

pub async fn spacetime_disconnected() {
    let server = TRACKMANIA.wait();
    if let Err(err) = server.pause_set_active(true).await {
        tracing::error!(
            "Error pausing the match in a critical disconnect section. Reason: {}",
            err
        )
    } else {
        tracing::info!("Server paused because of spacetime disconnect.");
    }
    if let Err(err) = server
            .chat_send_server_massage("$f00[tmservers.live] Disconnected abnormally. Entering recovery mode. An admin was notified and we are trying to reconnect ASAP. The match will resume once the situation is resolved. Sorry for the inconvenience.")
            .await
        {
            tracing::error!("Error sending disconnect message. Reason: {}", err)
        };

    loop {
        let connected = SPACETIME.connect(true).await;
        if connected {
            tracing::info!("Sucessfully reconnected.");
            if let Err(err) = server
            .chat_send_server_massage("$0f0[tmservers.live] Reconnected. Initiating seamless recovery. Match will resume in 20 seconds.")
            .await
        {
            tracing::error!("Error sending resume message. Reason: {}", err)
        };
            if let Err(err) = tokio::spawn(seamless_recovery()).await {
                tracing::error!("Error spawning seamless recovery. {err}")
            };
            return;
        } else {
            tracing::error!("Failed to reconnect. Retrying...");
        }
        sleep(Duration::from_secs(5)).await;
    }
}
