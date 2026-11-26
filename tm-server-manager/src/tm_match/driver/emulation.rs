//! The driver of a match, replayed from the records the bridge streams.
//!
//! The emulation only computes what the manager stores: the standings, the rounds which
//! counted and the laps of the races. What it would send to the server is dropped, the
//! bridge already did.

use std::collections::BTreeMap;

use tm_mode_driver::{DriverError, DriverLimits, RecordedDriver};
use tm_server_types::{
    config::DriverMode,
    event::{
        DriverHold, DriverRecord, DriverRecordDigest, DriverRoundResult, DriverSection, Event,
    },
    method::{DriverEntry, DriverTeamEntry},
};

/// The lap a player is in, as stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Lap {
    /// Of the player in the round, from 0.
    pub(crate) race: u16,
    /// Of the race, from 0.
    pub(crate) lap: u16,
    /// Race times of the checkpoints in this lap.
    pub(crate) checkpoints: Vec<u32>,
    pub(crate) finished: bool,
    pub(crate) respawns: u16,
    pub(crate) gave_up: bool,
}

/// A racer of the round that runs.
#[derive(Debug, Clone, Default)]
struct Racer {
    lap: Lap,
    /// Race times of the checkpoints of the race so far.
    race: Vec<u32>,
    /// The lap ended, the next checkpoint is in the next one.
    lap_ended: bool,
    /// The racer started a race in the round.
    started: bool,
}

/// What a record changed of what the manager stores.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Effect {
    /// The lap of a player changed, by the number of the round its race is in.
    Lap { player: u32, round: u16, lap: Lap },
    /// The standings of the match changed.
    Standings(Vec<DriverEntry>),
    /// The standings of the teams of a match of teams changed.
    TeamStandings(Vec<DriverTeamEntry>),
    /// A round counted, the number of rounds which counted so far and the round of the races.
    Round {
        number: u32,
        round: u16,
        result: DriverRoundResult,
    },
    /// The bridge computed other standings than the manager.
    Diverged(DriverRecordDigest),
}

pub(crate) struct Emulation {
    driver: RecordedDriver,
    /// Rounds which started outside a warmup, the round the races of the laps are in.
    round: u16,
    in_round: bool,
    in_warm_up: bool,
    racers: BTreeMap<u32, Racer>,
    /// The standings as stored, so only changes become effects.
    stored: Vec<DriverEntry>,
    stored_teams: Vec<DriverTeamEntry>,
}

/// The source of a mode.
fn source(mode: DriverMode) -> Box<dyn tm_mode_driver_sdk::ModeSource + Send + Sync> {
    match mode {
        DriverMode::BuiltIn(mode) => tm_mode_driver_builtin::builtin(mode),
    }
}

impl Emulation {
    pub(crate) fn start(mode: DriverMode, settings: &str) -> Result<Emulation, DriverError> {
        let source = source(mode);
        Ok(Emulation {
            driver: RecordedDriver::start(source.as_ref(), settings, DriverLimits::default())?,
            round: 0,
            in_round: false,
            in_warm_up: false,
            racers: BTreeMap::new(),
            stored: Vec::new(),
            stored_teams: Vec::new(),
        })
    }

    /// The account of a player number of the records.
    pub(crate) fn account(&self, player: u32) -> Option<&str> {
        self.driver.account(player)
    }

    /// Applies a record and returns what changed.
    pub(crate) fn apply(&mut self, record: &DriverRecord) -> Result<Vec<Effect>, DriverError> {
        let mut effects = Vec::new();
        self.races(record, &mut effects);

        let reply = self.driver.apply(record)?;
        if let Some(result) = reply.round_result {
            effects.push(Effect::Round {
                number: self.driver.driver().digest().rounds,
                round: self.round,
                result,
            });
        }
        if let DriverRecord::Digest(theirs) = record {
            let ours = self.driver.driver().digest();
            if ours != *theirs {
                effects.push(Effect::Diverged(*theirs));
            }
        }
        if changes_standings(record) {
            let standings = self.driver.driver().standings();
            if standings.entries != self.stored {
                self.stored.clone_from(&standings.entries);
                effects.push(Effect::Standings(standings.entries));
            }
            if standings.teams != self.stored_teams {
                self.stored_teams.clone_from(&standings.teams);
                effects.push(Effect::TeamStandings(standings.teams));
            }
        }
        Ok(effects)
    }

    /// Keeps the laps of the races of the round that runs.
    fn races(&mut self, record: &DriverRecord, effects: &mut Vec<Effect>) {
        match record {
            DriverRecord::Event(Event::StartRoundStart(_)) => {
                self.racers.clear();
                self.in_round = true;
                if !self.in_warm_up {
                    self.round = self.round.saturating_add(1);
                }
                return;
            }
            DriverRecord::Event(Event::WarmupStart) => self.in_warm_up = true,
            DriverRecord::Event(Event::WarmupEnd) => self.in_warm_up = false,
            DriverRecord::Event(Event::DriverHold(DriverHold {
                section: DriverSection::RoundEnd,
                ..
            })) => self.in_round = false,
            _ => {}
        }
        if !self.in_round || self.in_warm_up || self.round == 0 {
            return;
        }

        let (player, changed) = match record {
            DriverRecord::Start(start) => {
                let racer = self.racers.entry(start.player).or_default();
                new_race(racer, Vec::new());
                (start.player, true)
            }
            DriverRecord::Checkpoint(checkpoint) => {
                let racer = self.racers.entry(checkpoint.player).or_default();
                pass(racer, checkpoint.race_time);
                (checkpoint.player, true)
            }
            DriverRecord::LapEnd(lap_end) => {
                let racer = self.racers.entry(lap_end.player).or_default();
                pass(racer, lap_end.race_time);
                racer.lap_ended = true;
                (lap_end.player, true)
            }
            DriverRecord::Finish(finish) => {
                let racer = self.racers.entry(finish.player).or_default();
                pass(racer, finish.race_time);
                racer.lap.finished = true;
                (finish.player, true)
            }
            DriverRecord::Resync(resync) => {
                let racer = self.racers.entry(resync.player).or_default();
                let continues = racer.started
                    && !racer.lap.finished
                    && resync.checkpoints.starts_with(&racer.race);
                if continues {
                    // Checkpoints the server did not send belong to the race.
                    let missed = resync.checkpoints[racer.race.len()..].to_vec();
                    for race_time in missed {
                        pass(racer, race_time);
                    }
                } else {
                    // A race whose start the server did not send.
                    new_race(racer, resync.checkpoints.clone());
                }
                (resync.player, true)
            }
            DriverRecord::Respawn(player) => {
                let Some(racer) = self.racers.get_mut(player) else {
                    return;
                };
                racer.lap.respawns = racer.lap.respawns.saturating_add(1);
                (*player, true)
            }
            DriverRecord::GiveUp(player) => {
                let Some(racer) = self.racers.get_mut(player) else {
                    return;
                };
                racer.lap.gave_up = true;
                (*player, true)
            }
            _ => return,
        };
        if changed {
            effects.push(Effect::Lap {
                player,
                round: self.round,
                lap: self.racers[&player].lap.clone(),
            });
        }
    }
}

fn new_race(racer: &mut Racer, checkpoints: Vec<u32>) {
    let race = if racer.started {
        racer.lap.race.saturating_add(1)
    } else {
        0
    };
    racer.lap = Lap {
        race,
        lap: 0,
        checkpoints: checkpoints.clone(),
        ..Lap::default()
    };
    racer.race = checkpoints;
    racer.lap_ended = false;
    racer.started = true;
}

/// A checkpoint of the race of a racer, in a new race after a finish.
fn pass(racer: &mut Racer, race_time: u32) {
    if !racer.started || racer.lap.finished {
        new_race(racer, Vec::new());
    }
    if racer.lap_ended {
        racer.lap = Lap {
            race: racer.lap.race,
            lap: racer.lap.lap.saturating_add(1),
            ..Lap::default()
        };
        racer.lap_ended = false;
    }
    racer.lap.checkpoints.push(race_time);
    racer.race.push(race_time);
}

/// Whether the mode may have changed the standings, it only does when it gets stepped.
fn changes_standings(record: &DriverRecord) -> bool {
    !matches!(
        record,
        DriverRecord::Player(_)
            | DriverRecord::Checkpoint(_)
            | DriverRecord::LapEnd(_)
            | DriverRecord::Finish(_)
            | DriverRecord::Resync(_)
            | DriverRecord::Clock(_)
            | DriverRecord::Respawn(_)
            | DriverRecord::GiveUp(_)
            | DriverRecord::Digest(_)
    )
}
