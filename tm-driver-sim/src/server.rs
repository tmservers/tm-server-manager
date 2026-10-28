//! The fake server: the driver modescript, its players and the time they race in.
//!
//! It follows `TM_Driver.Script.txt` at the level of the callbacks the driver
//! handles. A match plays maps. A map waits for the driver when it starts, plays
//! the warmup and then rounds until the driver ends it, and waits at the end of
//! every round and of the map. The commands of the driver steer it like the real
//! script: the round format, the warmup, who can spawn, when maps and matches end
//! and which map comes next.
//!
//! Time is simulated in milliseconds. A race is planned when its player starts
//! and its checkpoints happen at their race times, so the callbacks of all players
//! interleave like on a server. Callbacks at the same moment come in a random
//! order. The finish timeout and the time limit of time attack rounds cut off the
//! races still running.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque},
};

use tm_server_types::{
    base::{Map, PlayerInfo},
    event::{
        DriverHold, DriverSection, EndMatch, Event, PlayerConnect, PlayerDisconnect, StartLine,
        StartMap, StartMatch, StartRound, WayPoint,
    },
    method::{DriverCommand, DriverFinishTimeout, DriverRoundFormat, DriverWarmUp},
};
use tm_wasm_driver::{DriverInput, DriverReply};

use crate::{
    Rng,
    world::{Action, MapSpec, Moment, RacePlan, Races, World},
};

/// From the start of a round until the players start racing.
const SPAWN: u32 = 1_500;
/// From the end of a race until the player starts again in time attack rounds.
const RESPAWN: u32 = 1_500;
/// Players who do not move give up after this long.
const IDLE: u32 = 30_000;
/// The modescript shows the scores this long after a round that counted.
const END_ROUND: u32 = 3_000;
/// The modescript asks again after a hold went unanswered this long.
const DECISION_TIMEOUT: u32 = 10_000;

/// What the simulation does next.
// Nearly every step is an input, boxing them would only add allocations.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Step {
    Input(DriverInput),
    /// The bridge restarts, so the driver resumes by replaying its inputs.
    RestartBridge,
}

/// What the driver could know about a round: the races as the callbacks that
/// reached it tell them.
#[derive(Debug, Clone, Default)]
pub struct RoundTruth {
    /// The racers in the order of their first start, with their races in order.
    pub racers: Vec<(String, Vec<SeenRace>)>,
    pub forced: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Load,
    StartMatch,
    StartMap,
    StartRound,
    Racing,
    EndMatch,
    Done,
}

/// A hold the modescript waits on.
struct Waiting {
    hold: DriverHold,
    continued: bool,
    deliveries: u32,
    /// Reaches the driver a second time.
    repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Round,
    /// A warmup round, with this many left after it.
    WarmUp {
        left: u32,
    },
}

/// Something that happens at a moment of a round.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Happening {
    Start {
        player: String,
        race: usize,
    },
    WayPoint {
        player: String,
        race: usize,
        checkpoint: usize,
    },
    /// A race ends without a finish.
    GiveUp {
        player: String,
        race: usize,
    },
    Join {
        player: String,
    },
    Leave {
        player: String,
    },
    ForceEnd,
    /// The finish timeout after the first finish or the time limit ran out.
    TimeUp,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Scheduled {
    time: u32,
    /// Orders happenings at the same moment at random.
    order: u64,
    happening: Happening,
}

#[derive(Debug, Default)]
struct Racer {
    races: Vec<RacePlan>,
    done: bool,
    gone: bool,
}

/// The round or warmup round that runs.
struct RoundRun {
    kind: Kind,
    /// When the round ends at the latest, e.g. by its time limit.
    end: Option<u32>,
    /// Players start again after their races.
    restarts: bool,
    finish_timeout: Option<u32>,
    first_finish: bool,
    forced_at_end: bool,
    queue: BinaryHeap<Reverse<Scheduled>>,
    racers: BTreeMap<String, Racer>,
}

pub(crate) struct Server {
    world: World,
    rng: Rng,
    clock: u32,
    phase: Phase,
    out: VecDeque<Step>,
    stopped: Option<String>,

    session: i32,
    seq: u32,
    hold: Option<Waiting>,
    repeat_holds: bool,

    // What the commands of the driver set, like the globals of the modescript.
    format: DriverRoundFormat,
    warm_up: DriverWarmUp,
    can_spawn: BTreeMap<String, bool>,
    end_map: bool,
    end_match: bool,
    next_map: Option<String>,

    connected: BTreeSet<String>,
    /// The name of every player the driver heard of.
    known: BTreeMap<String, String>,
    map: Option<usize>,
    matches_started: u32,
    matches_ended: u32,
    maps_in_match: u32,
    rounds_on_map: u32,
    /// Rounds of the simulation, without warmup rounds.
    rounds: u32,
    /// The round whose actions happened already.
    prepared: u32,
    restart_after: Option<u32>,
    round: Option<RoundRun>,
    /// The last round that ended.
    truth: RoundTruth,
    notes: Vec<String>,
}

impl Server {
    pub(crate) fn new(world: &World, seed: u64) -> Self {
        let rng = Rng::new(seed).fork("server");
        Self {
            world: world.clone(),
            session: session(&rng, 0),
            rng,
            clock: 0,
            phase: Phase::Load,
            out: VecDeque::new(),
            stopped: None,
            seq: 0,
            hold: None,
            repeat_holds: false,
            format: DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Automatic),
            warm_up: DriverWarmUp::default(),
            can_spawn: BTreeMap::new(),
            end_map: false,
            end_match: false,
            next_map: None,
            connected: BTreeSet::new(),
            known: BTreeMap::new(),
            map: None,
            matches_started: 0,
            matches_ended: 0,
            maps_in_match: 0,
            rounds_on_map: 0,
            rounds: 0,
            prepared: 0,
            restart_after: None,
            round: None,
            truth: RoundTruth::default(),
            notes: Vec::new(),
        }
    }

    /// Why the simulation ended before the last match did, if it did.
    pub(crate) fn stopped(&self) -> Option<&str> {
        self.stopped.as_deref()
    }

    pub(crate) fn can_spawn(&self, account_id: &str) -> bool {
        self.can_spawn.get(account_id) != Some(&false)
    }

    /// The name of every player the driver heard of.
    pub(crate) fn known(&self) -> &BTreeMap<String, String> {
        &self.known
    }

    pub(crate) fn map_uid(&self) -> &str {
        self.map
            .map_or("", |index| self.world.maps[index].uid.as_str())
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

    /// The races of the last round that ended.
    pub(crate) fn truth(&self) -> &RoundTruth {
        &self.truth
    }

    /// What happened around the last round, e.g. players joining, for the report.
    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    pub(crate) fn rounds(&self) -> u32 {
        self.rounds
    }

    pub(crate) fn next(&mut self) -> Option<Step> {
        loop {
            if let Some(step) = self.out.pop_front() {
                if let Step::Input(input) = &step {
                    self.learn(input);
                }
                return Some(step);
            }
            if let Some(waiting) = &mut self.hold {
                let hold = waiting.hold.clone();
                if !waiting.continued {
                    if waiting.deliveries >= 3 {
                        self.stop("The driver never let a hold continue");
                        return None;
                    }
                    waiting.deliveries += 1;
                    self.clock += DECISION_TIMEOUT;
                    self.deliver(DriverInput::Event(Event::DriverHold(hold)));
                    continue;
                }
                if waiting.repeat && waiting.deliveries == 1 {
                    waiting.deliveries += 1;
                    self.deliver(DriverInput::Event(Event::DriverHold(hold)));
                    continue;
                }
                self.hold = None;
                self.after_hold(&hold);
                continue;
            }

            match self.phase {
                Phase::Load => self.load(),
                Phase::StartMatch => self.start_match(),
                Phase::StartMap => self.start_map(),
                Phase::StartRound => self.start_round(),
                Phase::Racing => self.race(),
                Phase::EndMatch => self.end_match(),
                Phase::Done => return None,
            }
        }
    }

    /// Applies the commands of the driver like the modescript does.
    pub(crate) fn on_reply(&mut self, input: &DriverInput, reply: &DriverReply) {
        for command in &reply.commands {
            match command {
                DriverCommand::Continue(seq) => {
                    if let (DriverInput::Event(Event::DriverHold(hold)), Some(waiting)) =
                        (input, &mut self.hold)
                        && (waiting.hold.session, waiting.hold.seq) == (hold.session, hold.seq)
                        && *seq == hold.seq
                    {
                        waiting.continued = true;
                    }
                }
                DriverCommand::SetRoundFormat(format) => self.format = *format,
                DriverCommand::SetWarmUp(warm_up) => self.warm_up = *warm_up,
                DriverCommand::SetCanSpawn(can_spawn) => {
                    self.can_spawn
                        .insert(can_spawn.account_id.clone(), can_spawn.can_spawn);
                }
                DriverCommand::EndMap => self.end_map = true,
                DriverCommand::EndMatch => self.end_match = true,
                // The server refuses maps it does not have.
                DriverCommand::SetNextMap(uid) => {
                    if self.world.map(uid).is_some() {
                        self.next_map = Some(uid.clone());
                    }
                }
                DriverCommand::SetLayer(_)
                | DriverCommand::RemoveLayer(_)
                | DriverCommand::SetUiModuleVisible(_)
                | DriverCommand::SetNetState(_)
                | DriverCommand::SetStandings(_)
                | DriverCommand::SetRound(_)
                | DriverCommand::SetUsers(_)
                | DriverCommand::ChatMessage(_) => {}
            }
        }
    }

    fn stop(&mut self, reason: impl Into<String>) {
        self.stopped = Some(reason.into());
        self.phase = Phase::Done;
    }

    /// The names the driver knows once it got an input.
    fn learn(&mut self, input: &DriverInput) {
        match input {
            DriverInput::Players(players) => {
                for player in players.iter().filter(|player| !player.is_server()) {
                    self.known
                        .insert(player.account_id.clone(), player.nick_name.clone());
                }
            }
            DriverInput::Event(Event::PlayerInfoChanged(player)) => {
                self.known
                    .insert(player.account_id.clone(), player.nick_name.clone());
            }
            _ => {}
        }
    }

    /// Queues an input for the driver. The races of the round are recorded right
    /// away, their round end comes after them anyway.
    fn deliver(&mut self, input: DriverInput) {
        let in_round = self
            .round
            .as_ref()
            .is_some_and(|run| run.kind == Kind::Round);
        match &input {
            DriverInput::Event(Event::StartLine(start)) if in_round => {
                self.truth.start(&start.account_id);
            }
            DriverInput::Event(Event::WayPoint(way_point)) if in_round => {
                self.truth.way_point(
                    &way_point.account_id,
                    &way_point.current_race_checkpoints,
                    way_point.is_end_race,
                );
            }
            _ => {}
        }
        self.out.push_back(Step::Input(input));

        if let Some(after) = &mut self.restart_after {
            if *after == 0 {
                self.restart_after = None;
                self.notes.push("bridge restarts".into());
                self.out.push_back(Step::RestartBridge);
            } else {
                *after -= 1;
            }
        }
    }

    fn event(&mut self, event: Event) {
        self.deliver(DriverInput::Event(event));
    }

    fn emit_hold(&mut self, section: DriverSection, forced: bool) {
        self.seq += 1;
        let hold = DriverHold {
            session: self.session,
            seq: self.seq,
            section,
            forced,
        };
        let repeat = self.repeat_holds
            || self
                .rng
                .fork(&format!("repeat/{}/{}", self.session, self.seq))
                .chance(self.world.faults.repeated_holds);
        self.event(Event::DriverHold(hold.clone()));
        self.hold = Some(Waiting {
            hold,
            continued: false,
            deliveries: 1,
            repeat,
        });
    }

    /// What the modescript does once the driver let a hold continue.
    fn after_hold(&mut self, hold: &DriverHold) {
        match hold.section {
            DriverSection::MapStart => {
                if self.end_map || self.end_match {
                    self.emit_hold(DriverSection::MapEnd, false);
                } else if self.warm_up.rounds > 0 {
                    self.event(Event::WarmupStart);
                    self.plan_round(Kind::WarmUp {
                        left: self.warm_up.rounds - 1,
                    });
                } else {
                    self.phase = Phase::StartRound;
                }
            }
            DriverSection::RoundEnd => {
                if !hold.forced {
                    self.clock += END_ROUND;
                }
                if self.end_map || self.end_match {
                    self.emit_hold(DriverSection::MapEnd, false);
                } else {
                    self.phase = Phase::StartRound;
                }
            }
            DriverSection::MapEnd => {
                self.end_map = false;
                self.phase = if self.end_match {
                    Phase::EndMatch
                } else {
                    Phase::StartMap
                };
            }
        }
    }

    /// The driver loads: it gets the playlist and the players on the server, like
    /// the runner hands them over.
    fn load(&mut self) {
        if !self.world.playlist.is_empty() {
            self.deliver(DriverInput::Playlist(self.world.playlist.clone()));
        }
        let mut players = vec![server_player()];
        for player in self.world.players.iter().filter(|p| p.joins.is_none()) {
            self.connected.insert(player.account_id.clone());
            players.push(info(&self.world, &player.account_id));
        }
        self.deliver(DriverInput::Players(players));
        self.phase = Phase::StartMatch;
    }

    fn start_match(&mut self) {
        self.matches_started += 1;
        self.maps_in_match = 0;
        // Every match starts with the defaults of the modescript.
        self.format = DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Automatic);
        self.warm_up = DriverWarmUp::default();
        self.can_spawn.clear();
        self.end_map = false;
        self.end_match = false;
        self.event(Event::StartMatchStart(StartMatch {
            count: self.matches_started,
            time: self.clock,
        }));
        self.phase = Phase::StartMap;
    }

    fn start_map(&mut self) {
        let next = self
            .next_map
            .take()
            .and_then(|uid| self.world.maps.iter().position(|map| map.uid == uid));
        let index = match (next, self.map) {
            (Some(index), _) => index,
            (None, None) => 0,
            (None, Some(current)) => (current + 1) % self.world.maps.len(),
        };
        self.map = Some(index);
        self.maps_in_match += 1;
        self.rounds_on_map = 0;
        self.clock += 5_000;

        let spec = self.world.maps[index].clone();
        self.event(Event::StartMapStart(StartMap {
            count: self.maps_in_match,
            valid: 1,
            restarted: false,
            time: self.clock,
            map: map(&spec),
        }));
        self.emit_hold(DriverSection::MapStart, false);
    }

    fn start_round(&mut self) {
        let number = self.rounds + 1;
        if self.prepared < number {
            self.prepared = number;
            self.prepare(number);
            // A restarted modescript starts a new match first.
            if self.phase != Phase::StartRound {
                return;
            }
        }
        if number > self.world.max_rounds {
            self.stop(format!("Stopped after {} rounds", self.world.max_rounds));
            return;
        }
        if let Races::Scripted(rounds) = &self.world.races
            && number as usize > rounds.len()
        {
            self.stop("The scenario ends while the match goes on");
            return;
        }
        let single_attempt =
            !matches!(self.format, DriverRoundFormat::TimeAttack(limit) if limit > 0);
        let playing = self
            .connected
            .iter()
            .filter(|player| !self.watches(player))
            .count();
        if single_attempt && playing == 0 {
            // The modescript waits for players.
            self.stop("Nobody is on the server to play");
            return;
        }

        self.rounds = number;
        self.rounds_on_map += 1;
        self.clock += 1_000;
        self.event(Event::StartRoundStart(StartRound {
            count: self.rounds_on_map,
            time: self.clock,
        }));
        self.truth = RoundTruth::default();
        self.plan_round(Kind::Round);
    }

    /// What happens before a round of the simulation starts.
    fn prepare(&mut self, number: u32) {
        let actions = match &self.world.races {
            Races::Scripted(rounds) => rounds
                .get(number as usize - 1)
                .map(|round| round.before.clone())
                .unwrap_or_default(),
            Races::Random { .. } => self.random_actions(number),
        };
        for action in actions {
            match action {
                Action::Join(player) => self.join(&player),
                Action::Leave(player) => self.leave(&player),
                Action::RepeatHolds(repeat) => self.repeat_holds = repeat,
                Action::RestartBridge => {
                    self.notes.push("bridge restarts".into());
                    self.out.push_back(Step::RestartBridge);
                }
                Action::RestartScript => {
                    self.notes.push("modescript restarts".into());
                    self.session = session(&self.rng, number);
                    self.seq = 0;
                    // The script starts over on the map it was on.
                    self.next_map = self.map.map(|index| self.world.maps[index].uid.clone());
                    self.map = None;
                    self.phase = Phase::StartMatch;
                }
                Action::Playlist(uids) => {
                    self.notes.push(format!("playlist {}", uids.join(" ")));
                    self.deliver(DriverInput::Playlist(uids));
                }
            }
        }
    }

    /// Players coming and going and what goes wrong before a random round.
    fn random_actions(&mut self, number: u32) -> Vec<Action> {
        let mut actions = Vec::new();
        let before = Moment {
            round: number,
            during: false,
        };
        for player in &self.world.players {
            if player.joins == Some(before) {
                actions.push(Action::Join(player.account_id.clone()));
            }
            if player.leaves == Some(before) {
                actions.push(Action::Leave(player.account_id.clone()));
            }
        }

        let faults = &self.world.faults;
        let mut rng = self.rng.fork(&format!("faults/{number}"));
        if number > 1 && rng.chance(faults.script_restarts) {
            actions.push(Action::RestartScript);
        }
        if rng.chance(faults.playlist_changes) {
            let mut uids: Vec<String> = self.world.maps.iter().map(|map| map.uid.clone()).collect();
            rng.shuffle(&mut uids);
            uids.truncate(rng.below(uids.len() as u32 + 1) as usize);
            actions.push(Action::Playlist(uids));
        }
        if rng.chance(faults.bridge_restarts) {
            // Somewhere in the round.
            self.restart_after = Some(rng.below(40));
        }
        actions
    }

    fn watches(&self, account_id: &str) -> bool {
        self.world
            .player(account_id)
            .is_some_and(|player| player.watches)
    }

    fn join(&mut self, account_id: &str) {
        if !self.connected.insert(account_id.into()) {
            return;
        }
        self.notes.push(format!("{account_id} joins"));
        self.event(Event::PlayerConnect(PlayerConnect {
            account_id: account_id.into(),
            is_spectator: self.watches(account_id),
        }));
        let info = info(&self.world, account_id);
        self.event(Event::PlayerInfoChanged(info));
    }

    fn leave(&mut self, account_id: &str) {
        if !self.connected.remove(account_id) {
            return;
        }
        self.notes.push(format!("{account_id} leaves"));
        if let Some(racer) = self
            .round
            .as_mut()
            .and_then(|run| run.racers.get_mut(account_id))
        {
            racer.gone = true;
        }
        self.event(Event::PlayerDisconnect(PlayerDisconnect {
            account_id: account_id.into(),
            disconnect_reason: String::new(),
        }));
    }

    fn current_map(&self) -> MapSpec {
        self.world.maps[self.map.unwrap_or(0)].clone()
    }

    /// Starts a round or warmup round: everyone who can spawn starts racing.
    fn plan_round(&mut self, kind: Kind) {
        let map = self.current_map();
        let start = self.clock + SPAWN;
        let key = match kind {
            Kind::Round => format!("round/{}", self.rounds),
            Kind::WarmUp { left } => {
                format!(
                    "warmup/{}/{}/{left}",
                    self.matches_started, self.maps_in_match
                )
            }
        };
        let mut rng = self.rng.fork(&key);

        let (time_limit, finish_timeout) = match kind {
            Kind::Round => match self.format {
                DriverRoundFormat::TimeAttack(limit) if limit > 0 => (Some(limit), None),
                DriverRoundFormat::TimeAttack(_) => {
                    (None, timeout(DriverFinishTimeout::Automatic, &map))
                }
                DriverRoundFormat::SingleAttempt(finish_timeout) => {
                    (None, timeout(finish_timeout, &map))
                }
            },
            Kind::WarmUp { .. } => match self.warm_up.duration_ms {
                0 => (
                    None,
                    Some(
                        self.warm_up
                            .timeout_ms
                            .unwrap_or_else(|| automatic_timeout(&map)),
                    ),
                ),
                duration => (Some(duration), None),
            },
        };

        let mut run = RoundRun {
            kind,
            end: time_limit.map(|limit| start + limit),
            restarts: time_limit.is_some(),
            finish_timeout,
            first_finish: false,
            forced_at_end: false,
            queue: BinaryHeap::new(),
            racers: BTreeMap::new(),
        };

        let mut racers: Vec<String> = self
            .connected
            .iter()
            .filter(|player| !self.watches(player) && self.can_spawn(player))
            .cloned()
            .collect();
        rng.shuffle(&mut racers);
        for player in &racers {
            let plan = self.plan_race(kind, player, 0, &map, &mut rng.fork(player));
            run.racers.insert(
                player.clone(),
                Racer {
                    races: plan.into_iter().collect(),
                    ..Default::default()
                },
            );
        }
        if kind == Kind::Round {
            self.tie(&mut run, &racers, &mut rng);
        }
        for player in &racers {
            if !run.racers[player].races.is_empty() {
                schedule(
                    &mut run,
                    &mut rng,
                    start,
                    Happening::Start {
                        player: player.clone(),
                        race: 0,
                    },
                );
            } else {
                run.racers.get_mut(player).expect("Planned above").done = true;
            }
        }
        if let Some(end) = run.end {
            run.queue.push(Reverse(Scheduled {
                time: end,
                order: u64::MAX,
                happening: Happening::TimeUp,
            }));
        }

        if kind == Kind::Round {
            match &self.world.races {
                Races::Scripted(rounds) => {
                    run.forced_at_end = rounds[self.rounds as usize - 1].forced;
                }
                Races::Random { .. } => {
                    if rng.chance(self.world.faults.forced_rounds) {
                        let at = start + rng.below(map.author_time() * 3 / 2);
                        schedule(&mut run, &mut rng, at, Happening::ForceEnd);
                    }
                    let during = Moment {
                        round: self.rounds,
                        during: true,
                    };
                    for player in &self.world.players {
                        let at = start + rng.below(map.author_time());
                        if player.joins == Some(during) {
                            let player = player.account_id.clone();
                            schedule(&mut run, &mut rng, at, Happening::Join { player });
                        } else if player.leaves == Some(during) {
                            let player = player.account_id.clone();
                            schedule(&mut run, &mut rng, at, Happening::Leave { player });
                        }
                    }
                }
            }
        }

        self.round = Some(run);
        self.phase = Phase::Racing;
        // Nobody could spawn, so a round of one try ends right away.
        self.end_if_everyone_is_done();
    }

    /// The next race of a player, None when the player stops racing.
    fn plan_race(
        &self,
        kind: Kind,
        player: &str,
        race: usize,
        map: &MapSpec,
        rng: &mut Rng,
    ) -> Option<RacePlan> {
        match (&self.world.races, kind) {
            (Races::Random { idle, .. }, _) => {
                let spec = self.world.player(player)?;
                Some(spec.random_race(map, rng, *idle))
            }
            // Scripted players warm up once at the pace of the author.
            (Races::Scripted(_), Kind::WarmUp { .. }) => {
                (race == 0).then(|| RacePlan::Finish(map.author_checkpoints()))
            }
            (Races::Scripted(rounds), Kind::Round) => {
                let planned = rounds[self.rounds as usize - 1].races.get(player);
                match planned.and_then(|races| races.get(race)) {
                    Some(plan) => Some(plan.on(map)),
                    // Players without a race do not move.
                    None => (race == 0).then_some(RacePlan::Idle),
                }
            }
        }
    }

    /// Now and then two players drive the same race, or races with the same time,
    /// so the ties of the modes come up.
    fn tie(&self, run: &mut RoundRun, racers: &[String], rng: &mut Rng) {
        let Races::Random { ties, .. } = self.world.races else {
            return;
        };
        let finishers: Vec<&String> = racers
            .iter()
            .filter(|player| matches!(run.racers[*player].races.first(), Some(RacePlan::Finish(_))))
            .collect();
        if finishers.len() < 2 || !rng.chance(ties) {
            return;
        }
        let (first, second) = (finishers[0], finishers[1]);
        let mut race = run.racers[first].races[0].clone();
        if let RacePlan::Finish(checkpoints) = &mut race
            && rng.chance(500)
            && let RacePlan::Finish(other) = &run.racers[second].races[0]
            && other.len() == checkpoints.len()
        {
            // The same time with other checkpoint times.
            let finish = *checkpoints.last().expect("Races have a finish");
            *checkpoints = other.clone();
            if let Some(last) = checkpoints.last_mut() {
                *last = finish;
            }
            // Checkpoints have to stay in order.
            if checkpoints.windows(2).any(|pair| pair[0] >= pair[1]) {
                return;
            }
        }
        run.racers.get_mut(second).expect("A finisher").races[0] = race;
    }

    /// Lets the next happening of the round happen.
    fn race(&mut self) {
        let Some(run) = &mut self.round else {
            self.phase = Phase::StartRound;
            return;
        };
        let Some(Reverse(next)) = run.queue.pop() else {
            // Everyone is done, which ends the round.
            let forced = run.forced_at_end;
            self.end_round(forced);
            return;
        };
        self.clock = self.clock.max(next.time);

        match next.happening {
            Happening::Start { player, race } => self.start_race(&player, race),
            Happening::WayPoint {
                player,
                race,
                checkpoint,
            } => self.pass(&player, race, checkpoint),
            Happening::GiveUp { player, race } => self.race_over(&player, race),
            Happening::Join { player } => self.join(&player),
            Happening::Leave { player } => self.leave(&player),
            Happening::ForceEnd => {
                self.notes.push("forced to end".into());
                self.end_round(true);
                return;
            }
            Happening::TimeUp => {
                let forced = run_forced(&self.round);
                self.end_round(forced);
                return;
            }
        }
        self.end_if_everyone_is_done();
    }

    fn start_race(&mut self, player: &str, race: usize) {
        let now = self.clock;
        let run = self.round.as_mut().expect("A round runs");
        let racer = run.racers.get_mut(player).expect("A racer");
        if racer.gone || !self.connected.contains(player) {
            racer.done = true;
            return;
        }
        let plan = racer.races[race].clone();
        let key = format!("start/{}/{player}/{race}", self.rounds);
        let mut rng = self.rng.fork(&key);

        let missed = run.kind == Kind::Round && rng.chance(self.world.faults.missed_starts);
        let give_up = match &plan {
            RacePlan::Finish(checkpoints) | RacePlan::GiveUp(checkpoints) => {
                for (checkpoint, &time) in checkpoints.iter().enumerate() {
                    let happening = Happening::WayPoint {
                        player: player.into(),
                        race,
                        checkpoint,
                    };
                    schedule(run, &mut rng, now + time, happening);
                }
                match plan {
                    RacePlan::GiveUp(_) => {
                        Some(checkpoints.last().copied().unwrap_or(0) + rng.between(500, 5_000))
                    }
                    _ => None,
                }
            }
            RacePlan::Idle => Some(IDLE),
            RacePlan::FinishIn(_) | RacePlan::GiveUpAfter(_) => {
                unreachable!("Races are planned on their map")
            }
        };
        if let Some(after) = give_up {
            let happening = Happening::GiveUp {
                player: player.into(),
                race,
            };
            schedule(run, &mut rng, now + after, happening);
        }

        if missed {
            self.notes.push(format!("start of {player} missed"));
        } else {
            self.event(Event::StartLine(StartLine {
                account_id: player.into(),
                time: now,
            }));
        }
    }

    fn pass(&mut self, player: &str, race: usize, checkpoint: usize) {
        let now = self.clock;
        let run = self.round.as_mut().expect("A round runs");
        let racer = &run.racers[player];
        if racer.gone {
            return;
        }
        let (checkpoints, finished) = match &racer.races[race] {
            RacePlan::Finish(checkpoints) => {
                (checkpoints.clone(), checkpoint + 1 == checkpoints.len())
            }
            RacePlan::GiveUp(checkpoints) => (checkpoints.clone(), false),
            RacePlan::Idle => return,
            RacePlan::FinishIn(_) | RacePlan::GiveUpAfter(_) => {
                unreachable!("Races are planned on their map")
            }
        };
        let so_far = checkpoints[..=checkpoint].to_vec();
        let racetime = so_far[checkpoint];

        if finished && !run.first_finish && !run.restarts {
            run.first_finish = true;
            if let Some(timeout) = run.finish_timeout {
                run.queue.push(Reverse(Scheduled {
                    time: now + timeout,
                    order: u64::MAX,
                    happening: Happening::TimeUp,
                }));
            }
        }
        self.event(Event::WayPoint(WayPoint {
            account_id: player.into(),
            time: now,
            racetime,
            laptime: racetime,
            speed: 0.,
            checkpoint_in_race: checkpoint as u32,
            checkpoint_in_lap: checkpoint as u32,
            is_end_race: finished,
            is_end_lap: finished,
            is_infinite_laps: false,
            is_independent_laps: false,
            current_race_checkpoints: so_far.clone(),
            current_lap_checkpoints: so_far,
            block_id: String::new(),
        }));
        if finished {
            self.race_over(player, race);
        }
    }

    /// A player finished or gave up a race. In time attack rounds and warmups with a
    /// duration the player starts again.
    fn race_over(&mut self, player: &str, race: usize) {
        let now = self.clock;
        let map = self.current_map();
        let run = self.round.as_ref().expect("A round runs");
        let kind = run.kind;
        let again = run.restarts
            && run.end.is_some_and(|end| now + RESPAWN < end)
            && !run.racers[player].gone;
        let next = if again {
            let key = format!("race/{}/{player}/{}", self.rounds, race + 1);
            self.plan_race(kind, player, race + 1, &map, &mut self.rng.fork(&key))
        } else {
            None
        };

        let run = self.round.as_mut().expect("A round runs");
        let racer = run.racers.get_mut(player).expect("A racer");
        match next {
            Some(plan) => {
                racer.races.push(plan);
                let mut rng = self
                    .rng
                    .fork(&format!("respawn/{}/{player}/{race}", self.rounds));
                let happening = Happening::Start {
                    player: player.into(),
                    race: race + 1,
                };
                let at = now + RESPAWN + rng.below(1_000);
                schedule(run, &mut rng, at, happening);
            }
            None => racer.done = true,
        }
    }

    /// Rounds of one try end once nobody races anymore.
    fn end_if_everyone_is_done(&mut self) {
        let Some(run) = &self.round else {
            return;
        };
        if run.restarts || !run.racers.values().all(|racer| racer.done || racer.gone) {
            return;
        }
        let forced = run.forced_at_end;
        self.end_round(forced);
    }

    fn end_round(&mut self, forced: bool) {
        let run = self.round.take().expect("A round runs");
        // Players about to come or go still do.
        let mut pending: Vec<Scheduled> = run
            .queue
            .into_iter()
            .map(|Reverse(scheduled)| scheduled)
            .filter(|scheduled| {
                matches!(
                    scheduled.happening,
                    Happening::Join { .. } | Happening::Leave { .. }
                )
            })
            .collect();
        pending.sort();
        for scheduled in pending {
            match scheduled.happening {
                Happening::Join { player } => self.join(&player),
                Happening::Leave { player } => self.leave(&player),
                _ => {}
            }
        }

        match run.kind {
            Kind::Round => {
                self.truth.forced = forced;
                self.emit_hold(DriverSection::RoundEnd, forced);
            }
            Kind::WarmUp { left: 0 } => {
                self.event(Event::WarmupEnd);
                self.phase = Phase::StartRound;
            }
            Kind::WarmUp { left } => {
                self.clock += END_ROUND;
                self.plan_round(Kind::WarmUp { left: left - 1 });
            }
        }
    }

    fn end_match(&mut self) {
        self.matches_ended += 1;
        self.clock += 10_000;
        self.event(Event::EndMatchStart(EndMatch {
            count: self.matches_started,
            time: self.clock,
        }));
        self.phase = if self.matches_ended >= self.world.matches {
            Phase::Done
        } else {
            Phase::StartMatch
        };
    }
}

fn run_forced(round: &Option<RoundRun>) -> bool {
    round.as_ref().is_some_and(|run| run.forced_at_end)
}

fn schedule(run: &mut RoundRun, rng: &mut Rng, time: u32, happening: Happening) {
    run.queue.push(Reverse(Scheduled {
        time,
        order: rng.next_u64() >> 1,
        happening,
    }));
}

/// A random session for every start of the modescript.
fn session(rng: &Rng, start: u32) -> i32 {
    rng.fork(&format!("session/{start}"))
        .between(1, i32::MAX as u32 - 1) as i32
}

/// 5 seconds plus a sixth of the author time like the modescript.
fn automatic_timeout(map: &MapSpec) -> u32 {
    5_000 + map.author_time() / 6
}

fn timeout(finish_timeout: DriverFinishTimeout, map: &MapSpec) -> Option<u32> {
    match finish_timeout {
        DriverFinishTimeout::Automatic => Some(automatic_timeout(map)),
        DriverFinishTimeout::Disabled => None,
        DriverFinishTimeout::Milliseconds(ms) => Some(ms),
    }
}

fn map(spec: &MapSpec) -> Map {
    Map {
        uid: spec.uid.clone(),
        name: format!("Map {}", spec.uid),
        filename: format!("{}.Map.Gbx", spec.uid),
        author_account_id: String::new(),
        author_nickname: String::new(),
        environment: "Stadium".into(),
        mood: String::new(),
        bronze_time: 0,
        silver_time: 0,
        gold_time: 0,
        author_time: spec.author_time(),
        copperprice: 0,
        lap_race: false,
        number_laps: 0,
        map_type: r"TrackMania\TM_Race".into(),
        map_style: String::new(),
    }
}

fn info(world: &World, account_id: &str) -> PlayerInfo {
    let player = world.player(account_id).expect("A player of the world");
    PlayerInfo {
        account_id: account_id.into(),
        nick_name: player.name.clone(),
        player_id: 0,
        team_id: -1,
        spectator_status: i32::from(player.watches),
        ladder_ranking: 0,
        flags: 0,
    }
}

/// The server, which shows up as a player too.
fn server_player() -> PlayerInfo {
    PlayerInfo {
        account_id: "server".into(),
        nick_name: "Server".into(),
        player_id: 0,
        team_id: -1,
        spectator_status: 0,
        ladder_ranking: 0,
        flags: 0b10_0000,
    }
}
