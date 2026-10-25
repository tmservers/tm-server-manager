//! Runs the wasm mode controlling the generic driver modescript.

use std::sync::{Mutex, OnceLock};

use tm_server_controller::method::{ModeScriptMethodsXmlRpc, XmlRpcMethods};
use tm_server_types::{
    config::Driver as DriverConfig,
    event::{DriverEvent, Event},
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
}

/// Feeds the driver callbacks of the server into the active mode and sends its commands back.
pub fn setup_driver() {
    let (sender, mut receiver) = mpsc::unbounded_channel::<DriverEvent>();

    TRACKMANIA.wait().on_event(move |event| {
        if let Event::Driver(event) = event {
            _ = sender.send(event.clone());
        }
    });

    // A single task so the commands of every callback are sent in order.
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            let commands = {
                let mut active = ACTIVE.lock().unwrap();
                let Some(active) = active.as_mut() else {
                    tracing::warn!(
                        "Received {} but no driver mode is loaded.",
                        event.callback_name()
                    );
                    continue;
                };

                match active.driver.handle(&event) {
                    Ok(commands) => commands,
                    Err(error) => {
                        tracing::error!(
                            "Driver mode {} failed to handle {}: {error}",
                            active.mode,
                            event.callback_name()
                        );
                        continue;
                    }
                }
            };

            if commands.is_empty() {
                continue;
            }
            if let Err(error) = TRACKMANIA.wait().driver_commands(&commands).await {
                // Without the ack the modescript asks again and gets the same reply.
                tracing::error!("Could not send the driver commands: {error}");
            }
        }
    });
}

/// Loads the mode of a driver configuration.
/// Has to happen before the driver modescript gets loaded so no callback is missed.
pub async fn load(config: &DriverConfig) {
    match instantiate(config) {
        Ok(driver) => {
            tracing::info!("Loaded driver mode {}.", config.mode);
            *ACTIVE.lock().unwrap() = Some(ActiveDriver {
                mode: config.mode.clone(),
                driver,
            });
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

fn instantiate(config: &DriverConfig) -> Result<Driver, String> {
    // The name ends up in a file path.
    if config.mode.is_empty()
        || !config
            .mode
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
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

fn engine() -> Result<&'static DriverEngine, String> {
    if let Some(engine) = ENGINE.get() {
        return Ok(engine);
    }
    let engine = DriverEngine::new(DriverLimits::default()).map_err(|error| error.to_string())?;
    Ok(ENGINE.get_or_init(|| engine))
}
