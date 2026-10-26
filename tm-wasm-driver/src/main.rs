//! Drives a Trackmania server with a wasm mode, without the bridge and the manager.
//!
//! ```sh
//! TM_SERVER_URL=127.0.0.1:5000 cargo run -p tm-wasm-driver -- reverse-cup '{"starting_points": 10}'
//! ```
//!
//! Installs the driver modescript, shows the manialinks of `tm-driver-modes/manialinks` and
//! feeds the callbacks of the server into the mode. `scores-table` replaces the scores table
//! of the game. A mode is the name of a crate in
//! `tm-driver-modes`, which gets built first, or the path of a `.wasm` file. Settings are inline
//! json or the path of a json file.
//!
//! Commands while it runs:
//! - `load <mode> [settings]` starts a mode with a fresh state.
//! - `reload` builds and loads the last mode again.
//! - `script` installs the driver modescript again, which starts a new match.
//! - `next` ends the map.
//! - `quit`
//!
//! Environment:
//! - `TM_SERVER_URL` (required): XML-RPC address of the server.
//! - `TM_SUPERADMIN_PASSWORD` (default `SuperAdmin`).
//! - `TM_FILES`: UserData directory of the server. With it the script is written there and
//!   selected with `SetScriptName`, otherwise it is sent with `SetModeScriptText`.
//! - `RUST_LOG`, e.g. `debug` to see every callback the mode handles.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use tm_server_controller::{
    TrackmaniaServer,
    method::{ModeScriptMethodsXmlRpc, XmlRpcMethods},
};
use tm_server_types::{
    config::Driver as DriverConfig,
    event::Event,
    method::{DriverCommand, DriverLayer, DriverStandings, DriverStatus},
};
use tm_wasm_driver::{Driver, DriverEngine, DriverLimits};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing_subscriber::{EnvFilter, filter::LevelFilter};

const HELP: &str = "Commands: load <mode> [settings], reload, script, next, quit";

struct Active {
    /// The arguments of the `load` that started this mode.
    source: String,
    driver: Driver,
}

type Shared = Arc<Mutex<Option<Active>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let url = std::env::var("TM_SERVER_URL").unwrap_or("127.0.0.1:5001".into());

    let server: &'static TrackmaniaServer = Box::leak(Box::new(TrackmaniaServer::new(url).await?));
    let _: bool = server.call("SetApiVersion", "2023-03-25").await?;

    server.authenticate("SuperAdmin", "SuperAdmin").await?;

    let _: bool = server
        .call(
            "TriggerModeScriptEventArray",
            ("XmlRpc.SetApiVersion", ["3.11"]),
        )
        .await?;

    server.enable_callbacks(true).await?;
    server.enable_mode_script_callbacks(true).await?;

    tracing::info!("Connected to the server.");

    let engine: &'static DriverEngine =
        Box::leak(Box::new(DriverEngine::new(DriverLimits::default())?));
    let layers = load_layers()?;
    let active = Shared::default();
    drive(server, active.clone(), layers.clone());

    // The mode has to be ready before the modescript starts sending callbacks.
    let source = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if !source.is_empty()
        && let Err(error) = load(engine, server, &active, &layers, &source).await
    {
        tracing::error!("{error}");
    }
    // Mode script calls are refused until a mode script runs, so they come after the install.
    if let Err(error) = install_script(server).await {
        tracing::error!("{error}");
    }
    enable_mode_script_callbacks(server).await?;

    tracing::info!("{HELP}");
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        let (command, arguments) = line.split_once(' ').unwrap_or((line, ""));
        let result = match command {
            "" => Ok(()),
            "load" => load(engine, server, &active, &layers, arguments).await,
            "reload" => {
                let source = active.lock().unwrap().as_ref().map(|a| a.source.clone());
                match source {
                    Some(source) => load(engine, server, &active, &layers, &source).await,
                    None => Err("No mode was loaded yet.".into()),
                }
            }
            "script" => install_script(server).await,
            "next" => server
                .next_map()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "quit" | "exit" => break,
            _ => Err(format!("Unknown command {command}. {HELP}")),
        };
        if let Err(error) = result {
            tracing::error!("{error}");
        }
    }
    Ok(())
}

/// Feeds the callbacks into the active mode and sends its commands back, like the bridge.
fn drive(server: &'static TrackmaniaServer, active: Shared, layers: Vec<DriverLayer>) {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Event>();
    server.on_event(move |event| {
        if Driver::handles(event) || matches!(event, Event::StartServerEnd(_)) {
            _ = sender.send(event.clone());
        }
    });

    // A single task so the commands of every callback are sent in order.
    tokio::spawn(async move {
        while let Some(event) = receiver.recv().await {
            // A (re)started modescript lost the layers of the manialinks and shows the
            // scores table of the game again.
            if let Event::StartServerEnd(_) = event {
                send(server, DriverCommand::show_layers(&layers)).await;
                continue;
            }

            tracing::debug!("<- {event:?}");
            let (reply, standings) = {
                let mut active = active.lock().unwrap();
                let Some(active) = active.as_mut() else {
                    if matches!(event, Event::DriverHold(_)) {
                        tracing::warn!("The modescript waits for a mode. {HELP}");
                    }
                    continue;
                };
                match active.driver.handle(&event) {
                    Ok(reply) => {
                        let changed = reply
                            .commands
                            .iter()
                            .any(|c| matches!(c, DriverCommand::SetStandings(_)));
                        let standings = changed.then(|| active.driver.standings());
                        (reply, standings)
                    }
                    Err(error) => {
                        tracing::error!("The mode failed: {error}");
                        continue;
                    }
                }
            };

            if let Some(round_result) = reply.round_result {
                tracing::info!("Round result: {round_result:?}");
            }
            if let Some(standings) = standings {
                tracing::info!("Standings: {}", summary(&standings));
            }
            send(server, reply.commands).await;
        }
    });
}

/// Starts a mode with a fresh state. `source` is `<mode> [settings]`.
async fn load(
    engine: &DriverEngine,
    server: &TrackmaniaServer,
    active: &Shared,
    layers: &[DriverLayer],
    source: &str,
) -> Result<(), String> {
    let source = source.trim();
    let (mode, settings) = source
        .split_once(char::is_whitespace)
        .unwrap_or((source, ""));
    if mode.is_empty() {
        return Err(format!("Which mode? {HELP}"));
    }

    let wasm = build(mode).await?;
    let settings = read_settings(settings.trim())?;
    let module = engine.compile(&wasm).map_err(|e| e.to_string())?;
    let driver = engine
        .instantiate(&module, &settings)
        .map_err(|e| e.to_string())?;
    *active.lock().unwrap() = Some(Active {
        source: source.into(),
        driver,
    });
    tracing::info!("Loaded {mode}.");

    // Ignored until the driver modescript runs, which shows them once it started.
    send(server, DriverCommand::show_layers(layers)).await;
    Ok(())
}

/// Reads a `.wasm` file or builds the mode crate `tm-driver-mode-<mode>`.
async fn build(mode: &str) -> Result<Vec<u8>, String> {
    if mode.ends_with(".wasm") {
        return std::fs::read(mode).map_err(|e| format!("Could not read {mode}: {e}"));
    }

    let package = format!("tm-driver-mode-{mode}");
    let status = tokio::process::Command::new(std::env::var("CARGO").unwrap_or("cargo".into()))
        .current_dir(workspace())
        .args([
            "build",
            "-p",
            &package,
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .status()
        .await
        .map_err(|e| format!("Could not run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("Building {package} failed."));
    }

    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or(workspace().join("target"));
    let path = target
        .join("wasm32-unknown-unknown/release")
        .join(format!("{}.wasm", package.replace('-', "_")));
    std::fs::read(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))
}

/// Inline json or the path of a json file.
fn read_settings(settings: &str) -> Result<String, String> {
    if settings.is_empty() || settings.starts_with('{') {
        Ok(settings.into())
    } else {
        std::fs::read_to_string(settings).map_err(|e| format!("Could not read {settings}: {e}"))
    }
}

/// The server refuses mode script calls with "Not in script mode" until the script of the
/// server runs. Retries for a while so the install has time to take effect.
async fn enable_mode_script_callbacks(
    server: &TrackmaniaServer,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut attempts = 0;
    loop {
        let result: Result<bool, _> = server
            .call(
                "TriggerModeScriptEventArray",
                ("XmlRpc.SetApiVersion", ["3.11"]),
            )
            .await;
        match result {
            Ok(_) => break,
            Err(error) if attempts < 30 && error.to_string().contains("Not in script mode") => {
                attempts += 1;
                tracing::info!("Waiting for the server to run the modescript ({attempts}/30).");
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
    server.enable_mode_script_callbacks(true).await?;
    Ok(())
}

/// Starts the driver modescript of the repository, which also starts a new match.
async fn install_script(server: &TrackmaniaServer) -> Result<(), String> {
    let path = workspace().join("tm-server-types/external_modes/TM_Driver.Script.txt");
    let script = std::fs::read_to_string(&path)
        .map_err(|e| format!("Could not read {}: {e}", path.display()))?;

    match std::env::var("TM_FILES") {
        Ok(files) => {
            let file = Path::new(&files)
                .join("Scripts")
                .join(format!("{}.Script.txt", DriverConfig::SCRIPT_NAME));
            if let Some(directory) = file.parent() {
                std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
            }
            std::fs::write(&file, script)
                .map_err(|e| format!("Could not write {}: {e}", file.display()))?;
            server
                .set_script_name(DriverConfig::SCRIPT_NAME)
                .await
                .map_err(|e| e.to_string())?;
            // The script name only applies with the next map start.
            server.restart_map().await.map_err(|e| e.to_string())?;
        }
        Err(_) => {
            let _: bool = server
                .call("SetModeScriptText", script)
                .await
                .map_err(|e| format!("The server did not take the modescript: {e}"))?;
        }
    }
    tracing::info!("Installed the driver modescript.");
    Ok(())
}

/// Every manialink of `tm-driver-modes/manialinks`, named by its file.
fn load_layers() -> Result<Vec<DriverLayer>, String> {
    let directory = workspace().join("tm-driver-modes/manialinks");
    let mut layers = Vec::new();
    for entry in std::fs::read_dir(&directory)
        .map_err(|e| format!("Could not read {}: {e}", directory.display()))?
    {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|extension| extension == "xml") {
            layers.push(DriverLayer::selected(
                path.file_stem().unwrap().to_string_lossy(),
                std::fs::read_to_string(&path).map_err(|e| e.to_string())?,
            ));
        }
    }
    layers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(layers)
}

async fn send(server: &TrackmaniaServer, commands: Vec<DriverCommand>) {
    if commands.is_empty() {
        return;
    }
    let described: Vec<String> = commands.iter().map(describe).collect();
    tracing::info!("-> {}", described.join(", "));
    if let Err(error) = server.driver_commands(&commands).await {
        tracing::error!("Could not send the commands: {error}");
    }
}

/// A command for the log without the large json and manialinks.
fn describe(command: &DriverCommand) -> String {
    match command {
        DriverCommand::SetStandings(standings) => {
            tracing::debug!("Standings json: {standings}");
            format!("SetStandings({} bytes)", standings.len())
        }
        DriverCommand::SetNetState(state) => {
            tracing::debug!("Net state: {state}");
            format!("SetNetState({} bytes)", state.len())
        }
        DriverCommand::SetLayer(layer) => format!("SetLayer({})", layer.name),
        command => format!("{command:?}"),
    }
}

/// The standings in one line, e.g. `Rounds (2 / 5 rounds): 1. a 16 (+10), 2. b 10`.
fn summary(standings: &DriverStandings) -> String {
    let players: Vec<String> = standings
        .players
        .iter()
        .map(|player| {
            let score = match player.status {
                DriverStatus::Playing => match player.round_points {
                    0 => player.points.to_string(),
                    round => format!("{} ({round:+})", player.points),
                },
                status => format!("{status:?}"),
            };
            format!("{}. {} {score}", player.rank, player.name)
        })
        .collect();
    format!(
        "{} ({}): {}",
        standings.title,
        standings.info,
        players.join(", ")
    )
}

/// The repository, where the modescript, the modes and the manialinks live.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}
