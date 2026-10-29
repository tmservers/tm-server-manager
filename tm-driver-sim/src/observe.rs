//! What the checks and the report know about a match: every input the driver got
//! and the commands that reached the server. Simulations and logs are observed the
//! same way, so the checks of a simulation hold for any log of inputs, e.g. one
//! written by hand or recorded on a server.
//!
//! Inputs a log could not have come from a server, e.g. a race of a player who
//! cannot spawn, fail like a check, so a log that no longer fits a mode says so.

use std::collections::{BTreeMap, BTreeSet};

use tm_server_types::{
    event::{DriverHold, DriverSection, Event},
    method::{
        DriverCommand, DriverFinishTimeout, DriverRound, DriverRoundFormat, DriverStandings,
        DriverUsers, DriverWarmUp,
    },
};
use tm_wasm_driver::{DriverInput, DriverReply};

/// The races of the round that runs or ended last, as the callbacks that reached
/// the driver tell them.
#[derive(Debug, Clone, Default)]
pub struct RoundTruth {
    /// The racers in the order of their first start, with their races in order.
    pub racers: Vec<(String, Vec<SeenRace>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenRace {
    pub checkpoints: Vec<u32>,
    pub finished: bool,
}

impl SeenRace {
    pub fn time(&self) -> Option<u32> {
        self.finished
            .then(|| self.checkpoints.last().copied())
            .flatten()
    }
}

impl RoundTruth {
    pub fn races(&self, account_id: &str) -> Option<&[SeenRace]> {
        self.racers
            .iter()
            .find(|(racer, _)| racer == account_id)
            .map(|(_, races)| races.as_slice())
    }

    /// The time of the last finished race of a player.
    pub fn last_finish(&self, account_id: &str) -> Option<u32> {
        self.races(account_id)?
            .iter()
            .rev()
            .find_map(SeenRace::time)
    }

    /// The time of the fastest race of a player.
    pub fn best_finish(&self, account_id: &str) -> Option<u32> {
        self.races(account_id)?
            .iter()
            .filter_map(SeenRace::time)
            .min()
    }

    fn start(&mut self, account_id: &str) {
        self.races_mut(account_id).push(SeenRace {
            checkpoints: Vec::new(),
            finished: false,
        });
    }

    /// Like the host, a waypoint without a start still belongs to a race.
    fn way_point(&mut self, account_id: &str, checkpoints: &[u32], finished: bool) {
        let races = self.races_mut(account_id);
        if races.last().is_none_or(|race| race.finished) {
            races.push(SeenRace {
                checkpoints: Vec::new(),
                finished: false,
            });
        }
        let race = races.last_mut().expect("The player has a race");
        race.checkpoints = checkpoints.to_vec();
        race.finished = finished;
    }

    fn races_mut(&mut self, account_id: &str) -> &mut Vec<SeenRace> {
        let index = match self
            .racers
            .iter()
            .position(|(racer, _)| racer == account_id)
        {
            Some(index) => index,
            None => {
                self.racers.push((account_id.into(), Vec::new()));
                self.racers.len() - 1
            }
        };
        &mut self.racers[index].1
    }
}

/// A hold as it reached the driver.
#[derive(Debug, Clone)]
pub(crate) struct SeenHold {
    pub hold: DriverHold,
    /// The first time the hold reached the driver.
    pub first: bool,
    /// A round end that counts: neither forced nor without its round start.
    pub counts: bool,
    /// What the driver answered the first time, when the hold comes again.
    pub answered: Option<Vec<DriverCommand>>,
    /// The first answer reached the server, see `DriverInput::Delivered`.
    pub delivered: bool,
}

/// The first answer to a hold.
struct Answer {
    commands: Vec<DriverCommand>,
    delivered: bool,
}

/// What the manialinks of the server show, the last json the driver published.
#[derive(Debug, Default)]
pub(crate) struct Published {
    pub standings: Option<DriverStandings>,
    pub round: Option<DriverRound>,
    pub users: Option<DriverUsers>,
}

pub(crate) struct Observer {
    // What the driver could know from its inputs.
    known: BTreeMap<String, String>,
    connected: BTreeSet<String>,
    /// The driver got the players on the server once.
    players_seen: bool,
    truth: RoundTruth,
    in_round: bool,
    session: Option<i32>,
    seq: u32,
    answers: BTreeMap<(i32, u32), Answer>,
    /// The hold the driver answered last.
    last_hold: Option<(i32, u32)>,
    map: String,
    rounds: u32,
    /// The modescript said it is paused.
    paused: bool,
    /// The settings the mode plays with.
    settings: String,
    /// Settings that wait for the round that runs to end.
    pending_settings: Option<String>,
    /// The settings the mode took with the last input.
    changed_settings: Option<String>,

    // What the commands that reached the server set, like the globals of the
    // modescript and its netvars.
    can_spawn: BTreeMap<String, bool>,
    format: DriverRoundFormat,
    warm_up: DriverWarmUp,
    next_map: Option<String>,
    match_ended: bool,
    published: Published,

    notes: Vec<String>,
}

impl Observer {
    pub(crate) fn new(settings: &str) -> Self {
        Self {
            settings: settings.into(),
            pending_settings: None,
            changed_settings: None,
            known: BTreeMap::new(),
            connected: BTreeSet::new(),
            players_seen: false,
            truth: RoundTruth::default(),
            in_round: false,
            session: None,
            seq: 0,
            answers: BTreeMap::new(),
            last_hold: None,
            map: String::new(),
            rounds: 0,
            paused: false,
            can_spawn: BTreeMap::new(),
            format: default_format(),
            warm_up: DriverWarmUp::default(),
            next_map: None,
            match_ended: false,
            published: Published::default(),
            notes: Vec::new(),
        }
    }

    /// The name of every player the driver heard of.
    pub(crate) fn known(&self) -> &BTreeMap<String, String> {
        &self.known
    }

    /// The races of the round that runs or ended last.
    pub(crate) fn truth(&self) -> &RoundTruth {
        &self.truth
    }

    pub(crate) fn can_spawn(&self, account_id: &str) -> bool {
        self.can_spawn.get(account_id) != Some(&false)
    }

    pub(crate) fn published(&self) -> &Published {
        &self.published
    }

    pub(crate) fn map(&self) -> &str {
        &self.map
    }

    pub(crate) fn format(&self) -> DriverRoundFormat {
        self.format
    }

    pub(crate) fn warm_up(&self) -> DriverWarmUp {
        self.warm_up
    }

    pub(crate) fn next_map(&self) -> Option<&str> {
        self.next_map.as_deref()
    }

    pub(crate) fn paused(&self) -> bool {
        self.paused
    }

    /// The settings the mode plays with.
    pub(crate) fn settings(&self) -> &str {
        &self.settings
    }

    /// Settings that wait for the round that runs to end.
    pub(crate) fn pending_settings(&self) -> Option<&str> {
        self.pending_settings.as_deref()
    }

    /// The settings the mode took with the last input, before it got the input or
    /// with it, when the input were the settings.
    pub(crate) fn changed_settings(&self) -> Option<&str> {
        self.changed_settings.as_deref()
    }

    fn take_pending_settings(&mut self) {
        if let Some(settings) = self.pending_settings.take() {
            self.take_settings(settings);
        }
    }

    fn take_settings(&mut self, settings: String) {
        self.notes.push(format!("settings {settings}"));
        self.settings.clone_from(&settings);
        self.changed_settings = Some(settings);
    }

    /// Round ends that reached the driver, forced ones included.
    pub(crate) fn rounds(&self) -> u32 {
        self.rounds
    }

    /// What happened since the last call, e.g. players joining, for the report.
    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// Takes in an input before the driver gets it. Fails for an input the server
    /// could not have sent.
    pub(crate) fn input(&mut self, input: &DriverInput) -> Result<Option<SeenHold>, String> {
        self.changed_settings = None;
        // Like the host, settings that waited apply with the first input after the
        // round, before the mode gets it.
        if !self.in_round {
            self.take_pending_settings();
        }
        let event = match input {
            DriverInput::Event(event) => event,
            DriverInput::Playlist(uids) => {
                self.notes.push(format!("playlist {}", uids.join(" ")));
                return Ok(None);
            }
            DriverInput::Players(players) => {
                let connected: BTreeSet<String> = players
                    .iter()
                    .filter(|player| !player.is_server())
                    .map(|player| player.account_id.clone())
                    .collect();
                if std::mem::replace(&mut self.players_seen, true) {
                    for account_id in self.connected.difference(&connected) {
                        self.notes.push(format!("{account_id} is gone"));
                    }
                    for account_id in connected.difference(&self.connected) {
                        self.notes.push(format!("{account_id} is there"));
                    }
                }
                self.connected = connected;
                for player in players.iter().filter(|player| !player.is_server()) {
                    self.known
                        .insert(player.account_id.clone(), player.nick_name.clone());
                }
                return Ok(None);
            }
            DriverInput::Resumed | DriverInput::Settings(_) => return Ok(None),
            DriverInput::Delivered => {
                if let Some(answer) = self.last_hold.and_then(|hold| self.answers.get_mut(&hold)) {
                    answer.delivered = true;
                }
                return Ok(None);
            }
        };

        match event {
            Event::StartMatchStart(_) => {
                // Every match starts with the defaults of the modescript.
                self.match_ended = false;
                self.can_spawn.clear();
                self.format = default_format();
                self.warm_up = DriverWarmUp::default();
            }
            Event::StartMapStart(start) => {
                self.map.clone_from(&start.map.uid);
                self.next_map = None;
            }
            Event::StartRoundStart(_) => {
                if self.match_ended {
                    return Err("A round starts after the driver ended the match".into());
                }
                self.truth = RoundTruth::default();
                self.in_round = true;
            }
            Event::StartLine(start) => {
                if !self.can_spawn(&start.account_id) {
                    return Err(format!(
                        "{} starts a race but cannot spawn",
                        start.account_id
                    ));
                }
                self.truth.start(&start.account_id);
            }
            Event::WayPoint(way_point) => self.truth.way_point(
                &way_point.account_id,
                &way_point.current_race_checkpoints,
                way_point.is_end_race,
            ),
            Event::PlayerConnect(connect) => {
                if self.connected.insert(connect.account_id.clone()) {
                    self.notes.push(format!("{} joins", connect.account_id));
                }
            }
            Event::PlayerDisconnect(disconnect) => {
                if self.connected.remove(&disconnect.account_id) {
                    self.notes.push(format!("{} leaves", disconnect.account_id));
                }
            }
            Event::PlayerInfoChanged(player) if !player.is_server() => {
                self.known
                    .insert(player.account_id.clone(), player.nick_name.clone());
            }
            Event::Pause(pause) => {
                let paused = pause.available && pause.active;
                if paused != self.paused {
                    self.notes
                        .push(if paused { "paused" } else { "resumed" }.into());
                }
                self.paused = paused;
            }
            Event::DriverHold(hold) => return self.hold(hold).map(Some),
            _ => {}
        }
        Ok(None)
    }

    fn hold(&mut self, hold: &DriverHold) -> Result<SeenHold, String> {
        if self.session != Some(hold.session) {
            if self.session.is_some() {
                self.notes.push("modescript restarts".into());
            }
            // A restarted modescript is not paused.
            self.paused = false;
            self.session = Some(hold.session);
            self.seq = 0;
        }
        if hold.seq < self.seq {
            return Err(format!(
                "Hold {} comes after hold {} of the same modescript",
                hold.seq, self.seq
            ));
        }
        let answer = self.answers.get(&(hold.session, hold.seq));
        let answered = answer.map(|answer| answer.commands.clone());
        let delivered = answer.is_some_and(|answer| answer.delivered);
        let first = answered.is_none();
        self.seq = hold.seq;

        // Like the host, every new hold ends the round that ran.
        let in_round = first && std::mem::take(&mut self.in_round);
        let counts = first && hold.section == DriverSection::RoundEnd && !hold.forced && in_round;
        if first && hold.section == DriverSection::RoundEnd {
            self.rounds += 1;
        }

        Ok(SeenHold {
            hold: hold.clone(),
            first,
            counts,
            answered,
            delivered,
        })
    }

    /// Takes in the reply to an input. Only commands that reached the server change
    /// what it shows. Fails for json the driver published but nobody can read.
    pub(crate) fn reply(
        &mut self,
        input: &DriverInput,
        reply: &DriverReply,
        reached: bool,
    ) -> Result<(), String> {
        // The driver decided about new settings, whether its reply arrived or not.
        if let DriverInput::Settings(settings) = input {
            match &reply.rejected {
                Some(reason) => self.notes.push(format!("settings rejected: {reason}")),
                None if self.in_round => {
                    self.notes
                        .push("new settings wait for the round to end".into());
                    self.pending_settings = Some(settings.clone());
                }
                None => self.take_settings(settings.clone()),
            }
        }
        if let DriverInput::Event(Event::DriverHold(hold)) = input {
            self.answers
                .entry((hold.session, hold.seq))
                .or_insert_with(|| Answer {
                    commands: reply.commands.clone(),
                    delivered: false,
                });
            self.last_hold = Some((hold.session, hold.seq));
        }
        if !reached {
            return Ok(());
        }
        for command in &reply.commands {
            match command {
                DriverCommand::SetCanSpawn(can_spawn) => {
                    self.can_spawn
                        .insert(can_spawn.account_id.clone(), can_spawn.can_spawn);
                }
                DriverCommand::SetRoundFormat(format) => self.format = *format,
                DriverCommand::SetWarmUp(warm_up) => self.warm_up = *warm_up,
                DriverCommand::SetNextMap(uid) => self.next_map = Some(uid.clone()),
                DriverCommand::EndMatch => self.match_ended = true,
                DriverCommand::SetStandings(json) => self.published.standings = Some(parse(json)?),
                DriverCommand::SetRound(json) => self.published.round = Some(parse(json)?),
                DriverCommand::SetUsers(json) => self.published.users = Some(parse(json)?),
                DriverCommand::Continue(_)
                | DriverCommand::EndMap
                | DriverCommand::SetLayer(_)
                | DriverCommand::RemoveLayer(_)
                | DriverCommand::SetUiModuleVisible(_)
                | DriverCommand::SetNetState(_)
                | DriverCommand::ChatMessage(_) => {}
            }
        }
        Ok(())
    }
}

/// The round format of the modescript before the driver sets one.
fn default_format() -> DriverRoundFormat {
    DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Automatic)
}

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("Published json is invalid: {e}"))
}
