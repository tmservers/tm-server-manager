//! Runs the mode of a driver match next to the server.
//!
//! The driver decides without waiting for the database: it gets the callbacks of the
//! server, its commands go straight back. Every input goes through its records, which
//! stream to the manager one by one and numbered, so the manager replays the same
//! driver for the scores. The driver lives on a thread of its own, modes are no `Send`.

use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use tm_mode_driver::{DriverInput, DriverLimits, RecordingDriver};
use tm_server_controller::method::ModeScriptMethodsXmlRpc;
use tm_server_types::{
    base::PlayerInfo,
    config::{Driver as DriverConfig, DriverMode},
    event::{DriverRecord, Event},
    method::{DriverCommand, DriverLayer},
};
use tokio::sync::{Mutex, mpsc::UnboundedSender};

use crate::{SPACETIME, TRACKMANIA};

enum Message {
    Start {
        config: DriverConfig,
        playlist: Vec<String>,
        players: Vec<PlayerInfo>,
    },
    Stop,
    Input(DriverInput),
}

enum Out {
    /// A new run of the driver, its records count from 1.
    Session(u32),
    Apply {
        records: Vec<DriverRecord>,
        commands: Vec<DriverCommand>,
        /// The input was a hold, which learns whether its reply reached the server.
        hold: bool,
    },
}

static INBOX: OnceLock<mpsc::Sender<Message>> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// The connection to the manager came back, records may have been lost in between.
static RECONNECTED: AtomicBool = AtomicBool::new(false);

/// The records of the run of the driver, to send again what the manager missed.
static SENT: Mutex<(u32, Vec<DriverRecord>)> = Mutex::const_new((0, Vec::new()));

/// Whether a driver match runs, its races reach the manager as records then.
pub(crate) fn driver_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

pub(crate) fn setup_driver() {
    let (inbox, messages) = mpsc::channel::<Message>();
    let (outbox, mut out) = tokio::sync::mpsc::unbounded_channel::<Out>();
    std::thread::spawn(move || run(messages, outbox));
    _ = INBOX.set(inbox);

    // One task, so commands and records go out in the order the driver made them.
    tokio::spawn(async move {
        while let Some(out) = out.recv().await {
            match out {
                Out::Session(session) => *SENT.lock().await = (session, Vec::new()),
                Out::Apply {
                    records,
                    commands,
                    hold,
                } => {
                    stream(records).await;
                    let delivered = send(commands).await;
                    if hold && delivered {
                        input(DriverInput::Delivered);
                    }
                }
            }
        }
    });
}

/// A config was applied: a driver config starts its mode with a fresh state, any other stops it.
pub(crate) async fn driver_config(config: Option<&DriverConfig>, playlist: Vec<String>) {
    let Some(inbox) = INBOX.get() else {
        return;
    };
    let Some(config) = config else {
        ACTIVE.store(false, Ordering::Relaxed);
        _ = inbox.send(Message::Stop);
        return;
    };
    // Players already on the server do not connect again.
    let players = match TRACKMANIA.wait().get_player_list().await {
        Ok(players) => players,
        Err(error) => {
            tracing::warn!("Could not get the players of the server: {error}");
            Vec::new()
        }
    };
    ACTIVE.store(true, Ordering::Relaxed);
    _ = inbox.send(Message::Start {
        config: config.clone(),
        playlist,
        players,
    });
}

/// A callback of the server for the driver.
pub(crate) fn driver_event(event: &Event) {
    let wanted = tm_mode_driver::Driver::handles(event)
        || matches!(
            event,
            Event::Respawn(_) | Event::GiveUp(_) | Event::StartServerEnd(_)
        );
    if driver_active() && wanted {
        input(DriverInput::Event(event.clone()));
    }
}

/// The connection to the manager came back.
pub(crate) fn driver_reconnected() {
    RECONNECTED.store(true, Ordering::Relaxed);
}

/// The manager got the records of the run up to `last_seq`. After a reconnect, the ones
/// after it go again. Otherwise they are on their way.
pub(crate) async fn driver_resend(session: u32, last_seq: u32) {
    if !RECONNECTED.swap(false, Ordering::Relaxed) {
        return;
    }
    let missing: Vec<(u32, DriverRecord)> = {
        let sent = SENT.lock().await;
        if sent.0 != session {
            return;
        }
        sent.1
            .iter()
            .enumerate()
            .skip(last_seq as usize)
            .map(|(index, record)| (index as u32 + 1, record.clone()))
            .collect()
    };
    for (seq, record) in missing {
        call(session, seq, record);
    }
}

fn input(input: DriverInput) {
    if let Some(inbox) = INBOX.get() {
        _ = inbox.send(Message::Input(input));
    }
}

/// The driver thread.
fn run(messages: mpsc::Receiver<Message>, out: UnboundedSender<Out>) {
    let mut active: Option<(RecordingDriver, Vec<DriverLayer>)> = None;
    for message in messages {
        match message {
            Message::Start {
                config,
                playlist,
                players,
            } => {
                active = None;
                let DriverMode::BuiltIn(mode) = config.mode;
                let source = tm_mode_driver_builtin::builtin(mode);
                let mut driver = match RecordingDriver::start(
                    source.as_ref(),
                    &config.settings,
                    DriverLimits::default(),
                ) {
                    Ok(driver) => driver,
                    Err(error) => {
                        tracing::error!("The mode of the driver did not start: {error}");
                        continue;
                    }
                };
                let layers = layers(&config.manialinks);
                _ = out.send(Out::Session(session()));
                let mut commands = DriverCommand::show_layers(&layers);
                let mut records = Vec::new();
                for input in [
                    DriverInput::Playlist(playlist),
                    DriverInput::Players(players),
                ] {
                    match driver.apply(&input) {
                        Ok((mut more, reply)) => {
                            records.append(&mut more);
                            commands.extend(reply.commands);
                        }
                        Err(error) => tracing::error!("The mode failed: {error}"),
                    }
                }
                _ = out.send(Out::Apply {
                    records,
                    commands,
                    hold: false,
                });
                active = Some((driver, layers));
            }
            Message::Stop => active = None,
            Message::Input(input) => {
                let Some((driver, layers)) = active.as_mut() else {
                    continue;
                };
                // A (re)started modescript lost the layers of the manialinks.
                if let DriverInput::Event(Event::StartServerEnd(_)) = input {
                    _ = out.send(Out::Apply {
                        records: Vec::new(),
                        commands: DriverCommand::show_layers(layers),
                        hold: false,
                    });
                    continue;
                }
                let hold = matches!(input, DriverInput::Event(Event::DriverHold(_)));
                match driver.apply(&input) {
                    Ok((records, reply)) => {
                        _ = out.send(Out::Apply {
                            records,
                            commands: reply.commands,
                            hold,
                        });
                    }
                    Err(error) => tracing::error!("The mode failed: {error}"),
                }
            }
        }
    }
}

/// The manialinks of the config, the ones which do not exist are left out.
fn layers(names: &[String]) -> Vec<DriverLayer> {
    names
        .iter()
        .filter_map(|name| match tm_mode_driver_manialinks::default(name) {
            Some(manialink) => Some(DriverLayer::selected(name.clone(), manialink.to_string())),
            None => {
                tracing::error!("There is no manialink {name}.");
                None
            }
        })
        .collect()
}

/// A number for a run of the driver, another one every time.
fn session() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    (nanos as u32).max(1)
}

async fn stream(records: Vec<DriverRecord>) {
    if records.is_empty() {
        return;
    }
    let mut sent = SENT.lock().await;
    let session = sent.0;
    for record in records {
        sent.1.push(record.clone());
        call(session, sent.1.len() as u32, record);
    }
}

fn call(session: u32, seq: u32, record: DriverRecord) {
    use tm_server_manager_api_rs::match_driver_record;
    // The generated type is the same one, so its encoding is as well.
    let record = spacetimedb_lib::bsatn::to_vec(&record)
        .ok()
        .and_then(|bytes| {
            spacetimedb_lib::bsatn::from_slice::<tm_server_manager_api_rs::DriverRecord>(&bytes)
                .ok()
        });
    let Some(record) = record else {
        tracing::error!("Record {seq} of the driver does not fit the manager.");
        return;
    };
    if let Err(error) = SPACETIME
        .read()
        .reducers
        .match_driver_record(session, seq, record)
    {
        tracing::error!("Could not stream record {seq} of the driver: {error}");
    }
}

/// Sends the commands of the driver to the server, true once they arrived.
async fn send(commands: Vec<DriverCommand>) -> bool {
    if commands.is_empty() {
        return true;
    }
    match TRACKMANIA.wait().driver_commands(&commands).await {
        Ok(_) => true,
        Err(error) => {
            tracing::error!("Could not send the commands of the driver: {error}");
            false
        }
    }
}
