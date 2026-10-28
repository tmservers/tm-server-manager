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
        DriverHold, DriverSection, EndMatch, Event, Pause, PlayerConnect, PlayerDisconnect,
        StartLine, StartMap, StartMatch, StartRound, WayPoint,
    },
    method::{DriverCommand, DriverFinishTimeout, DriverRoundFormat, DriverWarmUp},
};
use tm_wasm_driver::{DriverInput, DriverReply};

use crate::{
    Rng,
    world::{Action, MapSpec, Moment, RacePlan, World},
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
    /// The bridge is back up, so the driver resumes by replaying its inputs.
    RestartBridge,
    /// The bridge goes down after the input before, maybe before its reply went
    /// out. Only the server sees it.
    Crash {
        lose_reply: bool,
    },
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
    /// The modescript restarts before the reply arrives.
    restart: bool,
}

/// Where the modescript restarts in a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Restart {
    DuringRound,
    /// While its end waits for the driver, before the reply arrives.
    AtHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Round,
    /// A warmup round, with this many left after it.
    WarmUp {
        left: u32,
    },
    /// A pause, in which players drive without a round until it ends.
    Pause,
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
    /// An admin pauses the match, which skips the round.
    Pause,
    /// The modescript restarts, which ends the round without its end.
    RestartScript,
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
    /// Names the random streams of the round, e.g. `round/3/race/a/0`.
    key: String,
    /// When the round ends at the latest, e.g. by its time limit.
    end: Option<u32>,
    /// Players start again after their races.
    restarts: bool,
    finish_timeout: Option<u32>,
    first_finish: bool,
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
    failed: Option<String>,

    session: i32,
    seq: u32,
    hold: Option<Waiting>,

    // What the commands of the driver set, like the globals of the modescript.
    format: DriverRoundFormat,
    warm_up: DriverWarmUp,
    can_spawn: BTreeMap<String, bool>,
    end_map: bool,
    end_match: bool,
    next_map: Option<String>,
    paused: bool,

    connected: BTreeSet<String>,
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
    /// Where the modescript restarts in the round that comes next.
    restart: Option<Restart>,
    /// Restarts of the modescript so far.
    restarts: u32,
    /// The bridge is down and misses this many more callbacks it can miss.
    down: Option<u32>,
    round: Option<RoundRun>,
    /// What happened that the driver cannot tell, e.g. a start that did not reach it.
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
            failed: None,
            seq: 0,
            hold: None,
            format: DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Automatic),
            warm_up: DriverWarmUp::default(),
            can_spawn: BTreeMap::new(),
            end_map: false,
            end_match: false,
            next_map: None,
            paused: false,
            connected: BTreeSet::new(),
            map: None,
            matches_started: 0,
            matches_ended: 0,
            maps_in_match: 0,
            rounds_on_map: 0,
            rounds: 0,
            prepared: 0,
            restart_after: None,
            restart: None,
            restarts: 0,
            down: None,
            round: None,
            notes: Vec::new(),
        }
    }

    /// Why the simulation ended before the last match did, if it did.
    pub(crate) fn stopped(&self) -> Option<&str> {
        self.stopped.as_deref()
    }

    /// What went wrong that ended the simulation, e.g. a hold that never continued.
    pub(crate) fn failed(&self) -> Option<&str> {
        self.failed.as_deref()
    }

    /// What happened that the driver cannot tell, for the report.
    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    fn can_spawn(&self, account_id: &str) -> bool {
        self.can_spawn.get(account_id) != Some(&false)
    }

    pub(crate) fn next(&mut self) -> Option<Step> {
        loop {
            match self.out.pop_front() {
                Some(Step::Crash { .. }) => {
                    let down = self
                        .rng
                        .fork(&format!("down/{}/{}", self.rounds, self.clock))
                        .below(10);
                    self.notes.push("bridge goes down".into());
                    self.down = Some(down);
                    continue;
                }
                // Whatever the server sends while the bridge is down, also what it
                // queued before, misses the bridge or brings it back first.
                Some(Step::Input(input)) if self.down.is_some() => {
                    if let Some(down @ 1..) = &mut self.down
                        && missable(&input)
                    {
                        *down -= 1;
                        self.notes.push(format!("missed {}", describe(&input)));
                        continue;
                    }
                    self.down = None;
                    self.notes.push("bridge restarts".into());
                    let mut players = vec![server_player()];
                    for account_id in &self.connected {
                        players.push(info(&self.world, account_id));
                    }
                    self.out.push_front(Step::Input(input));
                    self.out
                        .push_front(Step::Input(DriverInput::Players(players)));
                    self.out.push_front(Step::Input(DriverInput::Resumed));
                    return Some(Step::RestartBridge);
                }
                Some(step) => return Some(step),
                None => {}
            }
            if let Some(waiting) = &mut self.hold {
                let hold = waiting.hold.clone();
                if !waiting.continued && waiting.restart {
                    self.hold = None;
                    self.notes
                        .push("the modescript restarts before the reply arrives".into());
                    self.restart_script();
                    continue;
                }
                if !waiting.continued {
                    if waiting.deliveries >= 3 {
                        self.failed = Some(format!(
                            "The driver never let hold {} continue",
                            waiting.hold.seq
                        ));
                        self.phase = Phase::Done;
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

    /// Applies the commands of the driver like the modescript does. The bridge then
    /// tells the driver that the reply to a hold reached the server. False when the
    /// reply never reached it because the bridge went down first.
    pub(crate) fn on_reply(&mut self, input: &DriverInput, reply: &DriverReply) -> bool {
        if matches!(self.out.front(), Some(Step::Crash { lose_reply: true })) {
            self.notes.push("the last reply is lost".into());
            return false;
        }
        // A modescript that restarts before the reply arrives never applies it.
        if let (DriverInput::Event(Event::DriverHold(hold)), Some(waiting)) = (input, &self.hold)
            && waiting.restart
            && (waiting.hold.session, waiting.hold.seq) == (hold.session, hold.seq)
        {
            return false;
        }
        if let DriverInput::Event(Event::DriverHold(_)) = input {
            self.out.push_front(Step::Input(DriverInput::Delivered));
        }
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
        true
    }

    fn stop(&mut self, reason: impl Into<String>) {
        self.stopped = Some(reason.into());
        self.phase = Phase::Done;
    }

    /// Queues an input for the driver. While the bridge is down, it misses race and
    /// player callbacks. It is back before any other callback, resumes and learns
    /// who is on the server.
    fn deliver(&mut self, input: DriverInput) {
        self.out.push_back(Step::Input(input));

        if let Some(after) = &mut self.restart_after {
            if *after == 0 {
                self.restart_after = None;
                let lose_reply = self
                    .rng
                    .fork(&format!("lose/{}/{}", self.rounds, self.clock))
                    .chance(500);
                self.out.push_back(Step::Crash { lose_reply });
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
        let repeat = self
            .rng
            .fork(&format!("repeat/{}/{}", self.session, self.seq))
            .chance(self.world.faults.repeated_holds);
        let restart = section == DriverSection::RoundEnd && self.restart == Some(Restart::AtHold);
        if restart {
            self.restart = None;
        }
        self.event(Event::DriverHold(hold.clone()));
        self.hold = Some(Waiting {
            hold,
            continued: false,
            deliveries: 1,
            repeat,
            restart,
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
                    self.resume();
                    self.emit_hold(DriverSection::MapEnd, false);
                } else if self.paused {
                    self.plan_round(Kind::Pause);
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
        self.plan_round(Kind::Round);
    }

    /// Players coming and going and what goes wrong before a round.
    fn prepare(&mut self, number: u32) {
        for action in self.random_actions(number) {
            match action {
                Action::Join(player) => self.join(&player),
                Action::Leave(player) => self.leave(&player),
                Action::RestartScript => self.restart_script(),
                Action::Playlist(uids) => self.deliver(DriverInput::Playlist(uids)),
            }
        }
    }

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
            match rng.below(3) {
                0 => actions.push(Action::RestartScript),
                1 => self.restart = Some(Restart::DuringRound),
                _ => self.restart = Some(Restart::AtHold),
            }
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
            Kind::Pause => format!("pause/{}", self.rounds),
        };
        // The order and times of happenings. Races draw from streams of their own, so
        // one racer more or less leaves the races of the others as they are.
        let mut rng = self.rng.fork(&key).fork("happenings");

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
            Kind::Pause => (Some(rng.between(20, 60) * 1_000), None),
        };

        let mut run = RoundRun {
            kind,
            key: key.clone(),
            end: time_limit.map(|limit| start + limit),
            restarts: time_limit.is_some(),
            finish_timeout,
            first_finish: false,
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
            let mut races = self.rng.fork(&format!("{key}/race/{player}/0"));
            let plan = self.plan_race(player, &map, &mut races);
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
            if self
                .rng
                .fork(&format!("{key}/pause"))
                .chance(self.world.faults.pauses)
            {
                let at = start + rng.below(map.author_time() * 3 / 2);
                schedule(&mut run, &mut rng, at, Happening::Pause);
            }
            if self.restart == Some(Restart::DuringRound) {
                self.restart = None;
                let at = start + rng.below(map.author_time());
                schedule(&mut run, &mut rng, at, Happening::RestartScript);
            }
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

        self.round = Some(run);
        self.phase = Phase::Racing;
        // Nobody could spawn, so a round of one try ends right away.
        self.end_if_everyone_is_done();
    }

    /// The next race of a player at the pace of the player.
    fn plan_race(&self, player: &str, map: &MapSpec, rng: &mut Rng) -> Option<RacePlan> {
        let spec = self.world.player(player)?;
        Some(spec.random_race(map, rng, self.world.races.idle))
    }

    /// Now and then two players drive the same race, or races with the same time,
    /// so the ties of the modes come up.
    fn tie(&self, run: &mut RoundRun, racers: &[String], rng: &mut Rng) {
        let finishers: Vec<&String> = racers
            .iter()
            .filter(|player| matches!(run.racers[*player].races.first(), Some(RacePlan::Finish(_))))
            .collect();
        if finishers.len() < 2 || !rng.chance(self.world.races.ties) {
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
            self.end_round(false);
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
                self.end_round(true);
                return;
            }
            Happening::Pause => {
                // The modescript answers the admin with its status and skips the round.
                self.paused = true;
                self.event(Event::Pause(Pause {
                    available: true,
                    active: true,
                }));
                self.end_round(true);
                return;
            }
            Happening::RestartScript => {
                self.leave_round();
                self.notes
                    .push("the modescript restarts during the round".into());
                self.restart_script();
                return;
            }
            Happening::TimeUp => {
                self.end_round(false);
                return;
            }
        }
        self.end_if_everyone_is_done();
    }

    fn start_race(&mut self, player: &str, race: usize) {
        let now = self.clock;
        let can_spawn = self.can_spawn(player);
        let connected = self.connected.contains(player);
        let run = self.round.as_mut().expect("A round runs");
        let racer = run.racers.get_mut(player).expect("A racer");
        if racer.gone || !connected || !can_spawn {
            racer.done = true;
            return;
        }
        let plan = racer.races[race].clone();
        let mut rng = self.rng.fork(&format!("{}/start/{player}/{race}", run.key));

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
        let again = run.restarts
            && run.end.is_some_and(|end| now + RESPAWN < end)
            && !run.racers[player].gone;
        let next = if again {
            let key = format!("{}/race/{player}/{}", run.key, race + 1);
            self.plan_race(player, &map, &mut self.rng.fork(&key))
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
                    .fork(&format!("{}/respawn/{player}/{race}", run.key));
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
        self.end_round(false);
    }

    /// The modescript starts over, which starts a new match on the map it was on.
    /// A round or hold that runs is gone.
    fn restart_script(&mut self) {
        self.restarts += 1;
        self.session = session(&self.rng, self.restarts);
        self.seq = 0;
        self.next_map = self.map.map(|index| self.world.maps[index].uid.clone());
        self.map = None;
        self.round = None;
        self.hold = None;
        self.paused = false;
        self.phase = Phase::StartMatch;
    }

    /// The pause ends, if one runs. The modescript answers the admin with its status.
    fn resume(&mut self) {
        if std::mem::take(&mut self.paused) {
            self.event(Event::Pause(Pause {
                available: true,
                active: false,
            }));
        }
    }

    fn end_round(&mut self, forced: bool) {
        let run = self.leave_round();
        match run.kind {
            Kind::Round => self.emit_hold(DriverSection::RoundEnd, forced),
            Kind::WarmUp { left: 0 } => {
                self.event(Event::WarmupEnd);
                self.phase = Phase::StartRound;
            }
            Kind::WarmUp { left } => {
                self.clock += END_ROUND;
                self.plan_round(Kind::WarmUp { left: left - 1 });
            }
            Kind::Pause => {
                self.resume();
                self.phase = Phase::StartRound;
            }
        }
    }

    /// Takes the round that ends. Players about to come or go still do.
    fn leave_round(&mut self) -> RoundRun {
        let mut run = self.round.take().expect("A round runs");
        let mut pending: Vec<Scheduled> = std::mem::take(&mut run.queue)
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
        run
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

/// Callbacks a bridge that is down can miss without the match going wrong: races
/// and players. The script asks again for holds, and the bridge is back before a
/// round, map or match starts.
fn missable(input: &DriverInput) -> bool {
    matches!(
        input,
        DriverInput::Event(
            Event::StartLine(_)
                | Event::WayPoint(_)
                | Event::PlayerConnect(_)
                | Event::PlayerDisconnect(_)
                | Event::PlayerInfoChanged(_)
        )
    )
}

/// A missed callback for the report.
fn describe(input: &DriverInput) -> String {
    match input {
        DriverInput::Event(Event::StartLine(start)) => format!("the start of {}", start.account_id),
        DriverInput::Event(Event::WayPoint(way_point)) if way_point.is_end_race => {
            format!("the finish of {}", way_point.account_id)
        }
        DriverInput::Event(Event::WayPoint(way_point)) => {
            format!("a checkpoint of {}", way_point.account_id)
        }
        DriverInput::Event(Event::PlayerConnect(connect)) => {
            format!("{} joining", connect.account_id)
        }
        DriverInput::Event(Event::PlayerDisconnect(disconnect)) => {
            format!("{} leaving", disconnect.account_id)
        }
        DriverInput::Event(Event::PlayerInfoChanged(player)) => {
            format!("the name of {}", player.account_id)
        }
        _ => "a callback".into(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Faults, PlayerSpec, Races};

    fn player(account_id: &str) -> PlayerSpec {
        PlayerSpec {
            account_id: account_id.into(),
            name: account_id.into(),
            pace: 1_000,
            spread: 1_000,
            give_up: 100,
            joins: None,
            leaves: None,
            watches: false,
        }
    }

    /// The races the server plans for the first round of a world with these players.
    fn first_round(accounts: &[&str]) -> BTreeMap<String, Vec<RacePlan>> {
        let world = World {
            settings: "{}".into(),
            maps: vec![MapSpec {
                uid: "m1".into(),
                sectors: vec![10_000, 10_000, 10_000],
            }],
            playlist: Vec::new(),
            players: accounts.iter().map(|account| player(account)).collect(),
            races: Races { ties: 0, idle: 0 },
            faults: Faults::default(),
            matches: 1,
            max_rounds: 1,
        };
        let mut server = Server::new(&world, 9);
        while server.phase != Phase::Racing {
            let Some(Step::Input(input)) = server.next() else {
                panic!("The first round never starts");
            };
            if let DriverInput::Event(Event::DriverHold(hold)) = &input {
                let reply = DriverReply {
                    commands: vec![DriverCommand::Continue(hold.seq)],
                    round_result: None,
                };
                server.on_reply(&input, &reply);
            }
        }
        let run = server.round.as_ref().expect("The round runs");
        run.racers
            .iter()
            .map(|(account, racer)| (account.clone(), racer.races.clone()))
            .collect()
    }

    #[test]
    fn another_racer_leaves_the_races_of_the_others() {
        let two = first_round(&["a", "b"]);
        let three = first_round(&["a", "b", "c"]);
        assert_eq!(two["a"], three["a"]);
        assert_eq!(two["b"], three["b"]);
    }
}
