//! Runs the wasm mode controlling the generic driver modescript.

use std::sync::{Mutex, OnceLock};

use tm_server_controller::{
    callbacks::TypedCallbacks,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tm_server_types::{
    config::Driver as DriverConfig,
    event::{DriverRoundResult, Event, StartServer},
    method::{DriverCommand, DriverLayer},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverLimits};
use tokio::sync::mpsc;

use crate::TRACKMANIA;

/// Compiling and instantiating modes is cheap compared to creating the engine.
static ENGINE: OnceLock<DriverEngine> = OnceLock::new();

/// The mode of the current configuration if it uses the driver modescript.
static ACTIVE: Mutex<Option<ActiveDriver>> = Mutex::new(None);

struct ActiveDriver {
    mode: String,
    driver: Driver,
    /// The manialinks selected next to the mode.
    layers: Vec<DriverLayer>,
}

/// Feeds the callbacks of the server into the active mode and sends its commands back.
pub fn setup_driver() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<Event>();
    let server = TRACKMANIA.wait();

    server.on_event(move |event| {
        if Driver::handles(event) {
            _ = sender.send(event.clone());
        }
    });

    // A (re)started modescript lost the layers of the manialinks and shows the
    // scores table of the game again.
    server.on_start_server_end(async |_: &StartServer| {
        let layers = ACTIVE
            .lock()
            .unwrap()
            .as_ref()
            .map(|active| active.layers.clone());
        if let Some(layers) = layers {
            send(DriverCommand::show_layers(&layers)).await;
        }
    });

    // A single task so the commands of every callback are sent in order.
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            let reply = {
                let mut active = ACTIVE.lock().unwrap();
                let Some(active) = active.as_mut() else {
                    // The standard callbacks also arrive while other modes run.
                    if matches!(event, Event::DriverHold(_)) {
                        tracing::warn!("The driver modescript waits but no driver mode is loaded.");
                    }
                    continue;
                };

                match active.driver.handle(&event) {
                    Ok(reply) => reply,
                    Err(error) => {
                        tracing::error!("Driver mode {} failed: {error}", active.mode);
                        continue;
                    }
                }
            };

            if let Some(round_result) = reply.round_result {
                report_round_result(round_result);
            }
            // Without the continue the modescript asks again and gets the same reply.
            send(reply.commands).await;
        }
    });
}

async fn send(commands: Vec<DriverCommand>) {
    if commands.is_empty() {
        return;
    }
    if let Err(error) = TRACKMANIA.wait().driver_commands(&commands).await {
        tracing::error!("Could not send the driver commands: {error}");
    }
}

/// Recording the results in the manager comes with its database integration.
fn report_round_result(round_result: DriverRoundResult) {
    tracing::info!("Round result of the driver: {round_result:?}");
}

/// Loads the mode of a driver configuration and shows its manialinks. The driver plays
/// the map pool of the configuration in order.
/// Has to happen before the driver modescript gets loaded so no callback is missed.
pub async fn load(config: &DriverConfig, maps: Vec<String>) {
    match instantiate(config) {
        Ok(mut driver) => {
            driver.set_playlist(maps);
            // Players already on the server do not connect again.
            let mut known_players = Vec::new();
            match TRACKMANIA.wait().get_player_list().await {
                Ok(players) => {
                    for player in players {
                        if let Ok(reply) = driver.handle(&Event::PlayerInfoChanged(player)) {
                            known_players.extend(reply.commands);
                        }
                    }
                }
                Err(error) => tracing::warn!("Could not get the players of the server: {error}"),
            }
            tracing::info!("Loaded driver mode {}.", config.mode);
            let layers = load_layers(&config.manialinks);

            // A running modescript keeps the layers of the previous driver mode.
            let mut commands: Vec<DriverCommand> = ACTIVE
                .lock()
                .unwrap()
                .take()
                .map(|previous| previous.layers)
                .unwrap_or_default()
                .into_iter()
                .filter(|previous| !layers.iter().any(|layer| layer.name == previous.name))
                .map(|previous| DriverCommand::RemoveLayer(previous.name))
                .collect();
            commands.extend(DriverCommand::show_layers(&layers));
            commands.extend(known_players);

            *ACTIVE.lock().unwrap() = Some(ActiveDriver {
                mode: config.mode.clone(),
                driver,
                layers,
            });
            // Ignored by other modescripts. The driver modescript gets them once it started.
            send(commands).await;
        }
        Err(error) => {
            tracing::error!("Could not load driver mode {}: {error}", config.mode);
            *ACTIVE.lock().unwrap() = None;
            // The modescript waits for decisions so the server stays paused until this is fixed.
            if let Err(error) = TRACKMANIA
                .wait()
                .chat_send_server_massage(format!(
                    "$f00[tmservers.live] Could not load the match mode {}. An admin needs to fix the configuration.",
                    config.mode
                ))
                .await
            {
                tracing::error!("Could not send the server message: {error}");
            }
        }
    }
}

/// Removes the active mode when the configuration does not use the driver modescript.
pub fn unload() {
    if let Some(active) = ACTIVE.lock().unwrap().take() {
        tracing::info!("Unloaded driver mode {}.", active.mode);
    }
}

/// Names end up in file paths.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn instantiate(config: &DriverConfig) -> Result<Driver, String> {
    if !valid_name(&config.mode) {
        return Err("Mode names may only contain letters, digits, - and _.".into());
    }

    let directory = std::env::var("TM_DRIVER_MODES_DIR").unwrap_or("./modes".into());
    let path = format!("{directory}/{}.wasm", config.mode);
    let wasm = std::fs::read(&path).map_err(|error| format!("Could not read {path}: {error}"))?;

    let engine = engine()?;
    let module = engine.compile(&wasm).map_err(|error| error.to_string())?;
    engine
        .instantiate(&module, &config.settings)
        .map_err(|error| error.to_string())
}

/// Missing manialinks only cost the UI, so they do not stop the mode.
fn load_layers(names: &[String]) -> Vec<DriverLayer> {
    let directory = std::env::var("TM_DRIVER_MANIALINKS_DIR").unwrap_or("./manialinks".into());
    names
        .iter()
        .filter_map(|name| {
            if !valid_name(name) {
                tracing::warn!("Skipping the manialink {name}. Names may only contain letters, digits, - and _.");
                return None;
            }
            let path = format!("{directory}/{name}.xml");
            match std::fs::read_to_string(&path) {
                Ok(manialink) => Some(DriverLayer::selected(name.clone(), manialink)),
                Err(error) => {
                    tracing::warn!("Could not read the manialink {path}: {error}");
                    None
                }
            }
        })
        .collect()
}

fn engine() -> Result<&'static DriverEngine, String> {
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let engine = DriverEngine::new(DriverLimits::default()).map_err(|error| error.to_string())?;
    Ok(ENGINE.get_or_init(|| engine))
}
