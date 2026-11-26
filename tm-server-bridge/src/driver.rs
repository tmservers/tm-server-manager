//! Runs the mode of a driver match next to the server.
//!
//! The driver decides without waiting for the database: it gets the callbacks of the
//! server, its commands go straight back. Every input goes through its records, which
//! stream to the manager one by one and numbered, so the manager replays the same
//! driver for the scores. The driver lives on a thread of its own, modes are no `Send`.

use std::collections::{BTreeMap, HashMap};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use tm_mode_driver::{DriverInput, DriverLimits, DriverUiAction, RecordingDriver};
use tm_server_controller::method::ModeScriptMethodsXmlRpc;
use tm_server_types::{
    base::PlayerInfo,
    config::{
        Driver as DriverConfig, DriverMode, DriverUi, Manialink, UiManifest, UiRoute, UiValue,
    },
    event::{DriverRecord, DriverTeam, DriverUiEvent, Event},
    method::{DriverCommand, DriverLayer, DriverNetUiData, DriverUiModule},
};
use tokio::sync::{Mutex, mpsc::UnboundedSender};

use crate::{SPACETIME, TRACKMANIA};

enum Message {
    Start {
        config: DriverConfig,
        playlist: Vec<String>,
        players: Vec<PlayerInfo>,
        teams: Vec<DriverTeam>,
        /// The manialinks of the competition the config shows, by revision.
        manialinks: BTreeMap<u32, (String, UiManifest)>,
    },
    Stop,
    Input(DriverInput),
    /// A player did something in a manialink.
    UiEvent(DriverUiEvent),
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
    /// An event of a manialink for the manager, parsed with its manifest.
    UiEvent {
        account_id: String,
        layer: String,
        event: u32,
        args: Vec<UiValue>,
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
                Out::UiEvent {
                    account_id,
                    layer,
                    event,
                    args,
                } => ui_event(account_id, layer, event, args),
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
    // The modescript of the config gets every value again.
    *UI_DATA_SENT.lock().await = BTreeMap::new();
    _ = inbox.send(Message::Start {
        config: config.clone(),
        playlist,
        players,
        teams: match_teams(),
        manialinks: server_manialinks(),
    });
    sync_ui_data().await;
}

/// A callback of the server for the driver.
pub(crate) fn driver_event(event: &Event) {
    let wanted = tm_mode_driver::Driver::handles(event)
        || matches!(
            event,
            Event::Respawn(_) | Event::GiveUp(_) | Event::StartServerEnd(_)
        );
    if !driver_active() {
        return;
    }
    if let Event::DriverUiEvent(ui_event) = event {
        if let Some(inbox) = INBOX.get() {
            _ = inbox.send(Message::UiEvent(ui_event.clone()));
        }
        return;
    }
    if let Event::StartServerEnd(_) = event {
        // A (re)started modescript lost the values of the manialinks.
        tokio::spawn(async {
            *UI_DATA_SENT.lock().await = BTreeMap::new();
            sync_ui_data().await;
        });
    }
    if wanted {
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

/// What a match shows in its manialinks, to parse what players do in them.
struct Shown {
    commands: Vec<DriverCommand>,
    ui: DriverUi,
    manifests: BTreeMap<u32, UiManifest>,
    teams: Vec<u32>,
    limit: RateLimit,
}

/// The driver thread.
fn run(messages: mpsc::Receiver<Message>, out: UnboundedSender<Out>) {
    let mut active: Option<(RecordingDriver, Shown)> = None;
    for message in messages {
        match message {
            Message::Start {
                config,
                playlist,
                players,
                teams,
                manialinks,
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
                let xml: BTreeMap<u32, String> = manialinks
                    .iter()
                    .map(|(revision, (xml, _))| (*revision, xml.clone()))
                    .collect();
                let layers = layers(&config.ui, &xml);
                let team_ids = teams.iter().map(|team| team.team_id).collect();
                _ = out.send(Out::Session(session()));
                let shown = show(&layers, &config.ui);
                let mut commands = shown.clone();
                let mut records = Vec::new();
                let mut inputs = vec![
                    DriverInput::Playlist(playlist),
                    DriverInput::Players(players),
                ];
                // A match of players has no teams.
                if !teams.is_empty() {
                    inputs.push(DriverInput::Teams(teams));
                }
                for input in inputs {
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
                active = Some((
                    driver,
                    Shown {
                        commands: shown,
                        ui: config.ui,
                        manifests: manialinks
                            .into_iter()
                            .map(|(revision, (_, manifest))| (revision, manifest))
                            .collect(),
                        teams: team_ids,
                        limit: RateLimit::default(),
                    },
                ));
            }
            Message::Stop => active = None,
            Message::UiEvent(ui_event) => {
                let Some((driver, shown)) = active.as_mut() else {
                    continue;
                };
                if !shown.limit.allow(&ui_event.account_id, Instant::now()) {
                    continue;
                }
                let users = driver.driver().users();
                let player = |account: &str| users.users.iter().any(|u| u.account_id == account);
                let team = |team: u32| shown.teams.contains(&team);
                match route(&shown.ui, &shown.manifests, &ui_event, player, team) {
                    Ok(Routed::Mode(action)) => {
                        match driver.apply(&DriverInput::UiAction(action)) {
                            Ok((records, reply)) => {
                                _ = out.send(Out::Apply {
                                    records,
                                    commands: reply.commands,
                                    hold: false,
                                });
                            }
                            Err(error) => tracing::error!("The mode failed: {error}"),
                        }
                    }
                    Ok(Routed::Manager { event, args }) => {
                        _ = out.send(Out::UiEvent {
                            account_id: ui_event.account_id,
                            layer: ui_event.layer,
                            event,
                            args,
                        });
                    }
                    Err(error) => tracing::debug!("Dropped an event of a manialink: {error}"),
                }
            }
            Message::Input(input) => {
                let Some((driver, shown)) = active.as_mut() else {
                    continue;
                };
                // A (re)started modescript lost the layers of the manialinks.
                if let DriverInput::Event(Event::StartServerEnd(_)) = input {
                    _ = out.send(Out::Apply {
                        records: Vec::new(),
                        commands: shown.commands.clone(),
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

/// The layers of the config with their manialinks.
fn layers(ui: &DriverUi, custom: &BTreeMap<u32, String>) -> Vec<DriverLayer> {
    ui.layers
        .iter()
        .filter_map(|layer| {
            let xml = match &layer.manialink {
                Manialink::BuiltIn(manialink) => {
                    tm_mode_driver_manialinks::default(manialink.name())
                }
                Manialink::Custom(revision) => custom.get(revision).map(String::as_str),
            };
            match xml {
                Some(xml) => Some(layer.layer(xml)),
                None => {
                    tracing::error!("There is no manialink {}.", layer.manialink.name());
                    None
                }
            }
        })
        .collect()
}

/// The layers, then the UI modules of the game as the config shows or hides them.
fn show(layers: &[DriverLayer], ui: &DriverUi) -> Vec<DriverCommand> {
    let mut commands = DriverCommand::show_layers(layers);
    commands.extend(ui.modules.iter().map(|module| {
        DriverCommand::SetUiModuleVisible(DriverUiModule {
            id: module.module.id().into(),
            visible: module.visible,
        })
    }));
    commands
}

/// The teams of the match of the server, as the manager knows them.
fn match_teams() -> Vec<DriverTeam> {
    use spacetimedb_sdk::Table;
    use tm_server_manager_api_rs::MyMatchTeamsTableAccess;
    SPACETIME
        .read()
        .db
        .my_match_teams()
        .iter()
        .filter_map(|team| {
            // The generated type is the same one, so its encoding is as well.
            let bytes = spacetimedb_lib::bsatn::to_vec(&team).ok()?;
            spacetimedb_lib::bsatn::from_slice::<DriverTeam>(&bytes).ok()
        })
        .collect()
}

/// The manialinks of the competition the config of the server shows, by revision.
fn server_manialinks() -> BTreeMap<u32, (String, UiManifest)> {
    use spacetimedb_sdk::Table;
    use tm_server_manager_api_rs::MyServerManialinksTableAccess;
    SPACETIME
        .read()
        .db
        .my_server_manialinks()
        .iter()
        .filter_map(|manialink| {
            let manifest = same(&manialink.manifest)?;
            Some((manialink.revision, (manialink.xml, manifest)))
        })
        .collect()
}

/// A generated type as the type it was generated from: their encoding is the same.
fn same<A: spacetimedb_lib::ser::Serialize, B: spacetimedb_lib::de::DeserializeOwned>(
    value: &A,
) -> Option<B> {
    let bytes = spacetimedb_lib::bsatn::to_vec(value).ok()?;
    spacetimedb_lib::bsatn::from_slice::<B>(&bytes).ok()
}

/// Where an event of a manialink goes, with its arguments parsed.
#[derive(Debug, PartialEq)]
enum Routed {
    Mode(DriverUiAction),
    Manager { event: u32, args: Vec<UiValue> },
}

/// Parses an event of a manialink of the config with the manifest of its manialink.
fn route(
    ui: &DriverUi,
    manifests: &BTreeMap<u32, UiManifest>,
    event: &DriverUiEvent,
    player: impl Fn(&str) -> bool,
    team: impl Fn(u32) -> bool,
) -> Result<Routed, String> {
    let Some(layer) = ui.layers.iter().find(|l| l.manialink.name() == event.layer) else {
        return Err(format!("The config shows no layer {}.", event.layer));
    };
    let manifest = match &layer.manialink {
        Manialink::BuiltIn(manialink) => manialink.manifest(),
        Manialink::Custom(revision) => manifests
            .get(revision)
            .cloned()
            .ok_or_else(|| format!("There is no manialink {revision}."))?,
    };
    let Some((index, spec)) = manifest.event(&event.name) else {
        return Err(format!("{} does not send {}.", event.layer, event.name));
    };
    let args = spec.parse(&event.data, player, team)?;
    Ok(match spec.route {
        UiRoute::Mode => Routed::Mode(DriverUiAction {
            account_id: event.account_id.clone(),
            layer: event.layer.clone(),
            event: spec.name.clone(),
            args,
        }),
        UiRoute::Manager => Routed::Manager { event: index, args },
    })
}

/// At most this many events of the manialinks per player and second reach the mode
/// and the manager, the modescript already limits them.
const UI_EVENTS_PER_SECOND: u32 = 10;

#[derive(Default)]
struct RateLimit(HashMap<String, (Instant, u32)>);

impl RateLimit {
    fn allow(&mut self, account_id: &str, now: Instant) -> bool {
        let (since, count) = self.0.entry(account_id.into()).or_insert((now, 0));
        if now.duration_since(*since) >= Duration::from_secs(1) {
            *since = now;
            *count = 0;
        }
        *count += 1;
        *count <= UI_EVENTS_PER_SECOND
    }
}

/// Hands an event of a manialink to the manager.
fn ui_event(account_id: String, layer: String, event: u32, args: Vec<UiValue>) {
    use tm_server_manager_api_rs::match_ui_event;
    let Some(args) = same(&args) else {
        tracing::error!("An event of {layer} does not fit the manager.");
        return;
    };
    if let Err(error) = SPACETIME
        .read()
        .reducers
        .match_ui_event(account_id, layer, event, args)
    {
        tracing::error!("Could not hand an event of a manialink to the manager: {error}");
    }
}

/// The values of the manialinks of the match as the modescript got them, by scope and key.
type UiDataKey = (String, u32, String, String);
static UI_DATA_SENT: Mutex<BTreeMap<UiDataKey, String>> = Mutex::const_new(BTreeMap::new());

/// The manager changed values of the manialinks of the match.
pub(crate) fn driver_ui_data_changed() {
    if driver_active() {
        tokio::spawn(sync_ui_data());
    }
}

/// Sends the values of the manager the modescript does not have yet.
async fn sync_ui_data() {
    let now: Vec<DriverNetUiData> = {
        use spacetimedb_sdk::Table;
        use tm_server_manager_api_rs::MyMatchUiDataTableAccess;
        SPACETIME
            .read()
            .db
            .my_match_ui_data()
            .iter()
            .filter_map(|data| same(&data))
            .collect()
    };
    let commands = {
        let mut sent = UI_DATA_SENT.lock().await;
        ui_data_changes(&mut sent, now)
    };
    send(commands).await;
}

/// The commands which bring what the modescript has to the values: changed ones, and
/// empty ones for keys which are gone.
fn ui_data_changes(
    sent: &mut BTreeMap<UiDataKey, String>,
    now: Vec<DriverNetUiData>,
) -> Vec<DriverCommand> {
    let mut current = BTreeMap::new();
    for data in now {
        for (key, value) in data.keys.into_iter().zip(data.values) {
            current.insert(
                (data.scope.clone(), data.team, data.account.clone(), key),
                value,
            );
        }
    }
    let command = |(scope, team, account, key): &UiDataKey, value: &str| {
        let data = DriverNetUiData {
            scope: scope.clone(),
            team: *team,
            account: account.clone(),
            keys: vec![key.clone()],
            values: vec![value.into()],
        };
        DriverCommand::SetUiData(serde_json::to_string(&data).expect("The values are json"))
    };
    let mut commands: Vec<DriverCommand> = sent
        .keys()
        .filter(|key| !current.contains_key(*key))
        .map(|key| command(key, ""))
        .collect();
    commands.extend(
        current
            .iter()
            .filter(|(key, value)| sent.get(*key) != Some(*value))
            .map(|(key, value)| command(key, value)),
    );
    *sent = current;
    commands
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

#[cfg(test)]
mod tests {
    use tm_server_types::config::{BuiltInManialink, UiArg, UiEventSpec, UiIntRange, UiLayer};

    use super::*;

    fn ui(manialinks: &[Manialink]) -> DriverUi {
        let mut ui = DriverUi::default();
        for manialink in manialinks {
            ui.layers.push(UiLayer::new(manialink.clone()));
        }
        ui
    }

    fn vote() -> UiManifest {
        UiManifest {
            events: vec![
                UiEventSpec {
                    name: "skip".into(),
                    route: UiRoute::Mode,
                    args: vec![UiArg::User],
                },
                UiEventSpec {
                    name: "vote".into(),
                    route: UiRoute::Manager,
                    args: vec![UiArg::Int(UiIntRange { min: 1, max: 5 }), UiArg::Team],
                },
            ],
            data_keys: Vec::new(),
        }
    }

    fn event(layer: &str, name: &str, data: &[&str]) -> DriverUiEvent {
        DriverUiEvent {
            layer: layer.into(),
            account_id: "a".into(),
            name: name.into(),
            data: data.iter().map(|d| d.to_string()).collect(),
        }
    }

    fn routed(ui: &DriverUi, event: &DriverUiEvent) -> Result<Routed, String> {
        let manifests = BTreeMap::from([(7, vote())]);
        route(
            ui,
            &manifests,
            event,
            |account| account == "b",
            |team| team == 2,
        )
    }

    #[test]
    fn events_go_where_their_manifest_sends_them() {
        let ui = ui(&[
            Manialink::Custom(7),
            Manialink::BuiltIn(BuiltInManialink::ReadyCheck),
        ]);

        assert_eq!(
            routed(&ui, &event("custom-7", "skip", &["b"])),
            Ok(Routed::Mode(DriverUiAction {
                account_id: "a".into(),
                layer: "custom-7".into(),
                event: "skip".into(),
                args: vec![UiValue::User("b".into())],
            }))
        );
        // The manager knows the event by its index in the manifest.
        assert_eq!(
            routed(&ui, &event("custom-7", "vote", &["4", "2"])),
            Ok(Routed::Manager {
                event: 1,
                args: vec![UiValue::Int(4), UiValue::Team(2)],
            })
        );
        assert_eq!(
            routed(&ui, &event("ready-check", "ready", &["1"])),
            Ok(Routed::Manager {
                event: 0,
                args: vec![UiValue::Bool(true)],
            })
        );
    }

    #[test]
    fn events_which_do_not_fit_are_dropped() {
        let ui = ui(&[Manialink::Custom(7), Manialink::Custom(8)]);
        let refused = [
            (
                event("custom-9", "vote", &["4", "2"]),
                "The config shows no layer custom-9.",
            ),
            (
                event("custom-8", "vote", &["4", "2"]),
                "There is no manialink 8.",
            ),
            (
                event("custom-7", "kick", &[]),
                "custom-7 does not send kick.",
            ),
            (
                event("custom-7", "skip", &["c"]),
                "Argument 1 of skip does not fit: c",
            ),
            (
                event("custom-7", "vote", &["4", "3"]),
                "Argument 2 of vote does not fit: 3",
            ),
            (
                event("custom-7", "vote", &["4"]),
                "vote takes 2 arguments, not 1",
            ),
        ];
        for (event, error) in refused {
            assert_eq!(routed(&ui, &event), Err(error.to_string()));
        }
    }

    #[test]
    fn players_send_a_few_events_a_second() {
        let mut limit = RateLimit::default();
        let start = Instant::now();
        for _ in 0..UI_EVENTS_PER_SECOND {
            assert!(limit.allow("a", start));
        }
        assert!(!limit.allow("a", start + Duration::from_millis(999)));
        // Others still can, and a second later the player again.
        assert!(limit.allow("b", start));
        assert!(limit.allow("a", start + Duration::from_secs(1)));
    }

    #[test]
    fn values_go_out_when_they_change_and_empty_once_gone() {
        let data = |scope: &str, account: &str, key: &str, value: &str| DriverNetUiData {
            scope: scope.into(),
            team: 0,
            account: account.into(),
            keys: vec![key.into()],
            values: vec![value.into()],
        };
        let sent = |commands: Vec<DriverCommand>| -> Vec<DriverNetUiData> {
            commands
                .into_iter()
                .map(|command| match command {
                    DriverCommand::SetUiData(json) => serde_json::from_str(&json).unwrap(),
                    other => panic!("{other:?} is no value"),
                })
                .collect()
        };
        let mut state = BTreeMap::new();

        let first = vec![
            data("all", "", "count", "1/2"),
            data("account", "a", "ready", "1"),
        ];
        assert_eq!(
            sent(ui_data_changes(&mut state, first.clone())),
            [
                data("account", "a", "ready", "1"),
                data("all", "", "count", "1/2")
            ]
        );
        assert!(ui_data_changes(&mut state, first).is_empty());

        let second = vec![data("all", "", "count", "0/2")];
        assert_eq!(
            sent(ui_data_changes(&mut state, second)),
            [
                data("account", "a", "ready", ""),
                data("all", "", "count", "0/2")
            ]
        );
        // What was sent is forgotten for a new modescript, which gets everything.
        let mut fresh = BTreeMap::new();
        assert_eq!(
            sent(ui_data_changes(
                &mut fresh,
                vec![data("all", "", "count", "0/2")]
            )),
            [data("all", "", "count", "0/2")]
        );
    }
}
