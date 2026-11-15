//! The records of the inputs of a driver, as the bridge streams them to the manager.
//!
//! A [`Recorder`] turns the inputs of the bridge into [`DriverRecord`]s, a
//! [`RecordedDriver`] turns them back into inputs and applies them. The bridge and
//! the manager both run a recorded driver, so both get exactly the same inputs, even
//! where the records leave out what a callback carried. A start and a checkpoint
//! only carry what is new about the race, the replay adds the checkpoints before. When
//! that would not give the race the server sent, e.g. after a lost waypoint, the
//! recorder sends the race as it is first.

use std::collections::BTreeMap;

use tm_mode_driver_sdk::ModeSource;
use tm_server_types::event::{
    DriverRecord, DriverRecordCheckpoint, DriverRecordDigest, DriverRecordLapEnd,
    DriverRecordPlayer, DriverRecordResync, DriverRecordStart, Event, StartLine, WayPoint,
};

use crate::{Driver, DriverError, DriverInput, DriverLimits, DriverReply};

/// What the replay knows about the race of a player.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RaceState {
    /// Server time the race started.
    start: u32,
    /// Race times of the checkpoints so far.
    checkpoints: Vec<u32>,
    finished: bool,
    /// The start of this race is known, from a start or a resync.
    known: bool,
}

/// Turns records back into the inputs of a driver.
#[derive(Debug, Default, Clone)]
pub struct Replayer {
    accounts: Vec<String>,
    races: BTreeMap<u32, RaceState>,
}

impl Replayer {
    /// The account of a player number.
    pub fn account(&self, player: u32) -> Option<&str> {
        self.accounts.get(player as usize).map(String::as_str)
    }

    /// The input a record is, none for records which only carry statistics or what
    /// the replay needs to know.
    pub fn input(&mut self, record: &DriverRecord) -> Option<DriverInput> {
        match record {
            DriverRecord::Event(event) => {
                self.event(event);
                Some(DriverInput::Event(event.clone()))
            }
            DriverRecord::Player(DriverRecordPlayer { player, account_id }) => {
                let index = *player as usize;
                if index >= self.accounts.len() {
                    self.accounts.resize(index + 1, String::new());
                }
                self.accounts[index].clone_from(account_id);
                None
            }
            DriverRecord::Start(DriverRecordStart { player, time }) => {
                let account_id = self.account(*player)?.to_string();
                let race = self.races.entry(*player).or_default();
                *race = RaceState {
                    start: *time,
                    checkpoints: Vec::new(),
                    finished: false,
                    known: true,
                };
                Some(DriverInput::Event(Event::StartLine(StartLine {
                    account_id,
                    time: *time,
                })))
            }
            DriverRecord::Checkpoint(DriverRecordCheckpoint {
                player,
                race_time,
                index,
            }) => self.way_point(*player, *race_time, *index, None, false),
            DriverRecord::LapEnd(lap) => self.way_point(
                lap.player,
                lap.race_time,
                lap.index,
                Some(lap.lap_length),
                false,
            ),
            DriverRecord::Finish(finish) => {
                let lap_length = (finish.lap_length > 0).then_some(finish.lap_length);
                self.way_point(
                    finish.player,
                    finish.race_time,
                    finish.index,
                    lap_length,
                    true,
                )
            }
            DriverRecord::Resync(DriverRecordResync {
                player,
                start,
                checkpoints,
            }) => {
                self.races.insert(
                    *player,
                    RaceState {
                        start: *start,
                        checkpoints: checkpoints.clone(),
                        finished: false,
                        known: true,
                    },
                );
                None
            }
            DriverRecord::Clock(DriverRecordStart { player, time }) => {
                let race = self.races.entry(*player).or_default();
                race.start = *time;
                None
            }
            DriverRecord::Respawn(_) | DriverRecord::GiveUp(_) | DriverRecord::Digest(_) => None,
            DriverRecord::Playlist(maps) => Some(DriverInput::Playlist(maps.clone())),
            DriverRecord::Players(players) => Some(DriverInput::Players(players.clone())),
            DriverRecord::Delivered => Some(DriverInput::Delivered),
            DriverRecord::Resumed => Some(DriverInput::Resumed),
            DriverRecord::Settings(settings) => Some(DriverInput::Settings(settings.clone())),
            DriverRecord::Teams(teams) => Some(DriverInput::Teams(teams.clone())),
        }
    }

    /// Callbacks recorded as they came still tell about races.
    fn event(&mut self, event: &Event) {
        match event {
            // Every race of a round starts in it.
            Event::StartRoundStart(_) => self.races.clear(),
            Event::StartLine(start) => {
                if let Some(player) = self.number(&start.account_id) {
                    self.races.insert(
                        player,
                        RaceState {
                            start: start.time,
                            checkpoints: Vec::new(),
                            finished: false,
                            known: true,
                        },
                    );
                }
            }
            Event::WayPoint(way_point) => {
                if let Some(player) = self.number(&way_point.account_id) {
                    self.races.insert(
                        player,
                        RaceState {
                            start: way_point.time.wrapping_sub(way_point.racetime),
                            checkpoints: way_point.current_race_checkpoints.clone(),
                            finished: way_point.is_end_race,
                            known: true,
                        },
                    );
                }
            }
            _ => {}
        }
    }

    fn number(&self, account_id: &str) -> Option<u32> {
        self.accounts
            .iter()
            .position(|account| account == account_id)
            .map(|index| index as u32)
    }

    fn way_point(
        &mut self,
        player: u32,
        race_time: u32,
        index: u16,
        lap_length: Option<u16>,
        finish: bool,
    ) -> Option<DriverInput> {
        let account_id = self.account(player)?.to_string();
        let race = self.races.entry(player).or_default();
        // Only a new race comes after the finish.
        if race.finished {
            *race = RaceState::default();
        }
        race.checkpoints.push(race_time);
        race.finished = finish;
        Some(DriverInput::Event(Event::WayPoint(WayPoint {
            account_id,
            time: race.start.wrapping_add(race_time),
            racetime: race_time,
            laptime: 0,
            speed: 0.,
            checkpoint_in_race: u32::from(index),
            checkpoint_in_lap: lap_length.map_or(0, |length| u32::from(length.saturating_sub(1))),
            is_end_race: finish,
            is_end_lap: lap_length.is_some(),
            is_infinite_laps: false,
            is_independent_laps: false,
            current_race_checkpoints: race.checkpoints.clone(),
            current_lap_checkpoints: Vec::new(),
            block_id: String::new(),
        })))
    }

    /// What a waypoint recorded compactly would replay as: its checkpoints and its
    /// server time, none when the start of its race is not known.
    fn predicts(&self, player: u32, race_time: u32) -> (Vec<u32>, Option<u32>) {
        match self.races.get(&player) {
            Some(race) if !race.finished => {
                let mut checkpoints = race.checkpoints.clone();
                checkpoints.push(race_time);
                let time = race.known.then(|| race.start.wrapping_add(race_time));
                (checkpoints, time)
            }
            _ => (vec![race_time], None),
        }
    }
}

/// Turns the inputs of a driver into records, see the module.
#[derive(Debug, Default, Clone)]
pub struct Recorder {
    numbers: BTreeMap<String, u32>,
    /// How the records replay, so the recorder knows when they would not.
    replay: Replayer,
}

impl Recorder {
    /// The records of an input, in order. Callbacks the driver does not handle give
    /// none, except respawns and give ups, for the statistics of the race.
    pub fn record(&mut self, input: &DriverInput) -> Vec<DriverRecord> {
        let mut records = Vec::new();
        match input {
            DriverInput::Event(Event::StartLine(start)) => {
                let player = self.number(&start.account_id, &mut records);
                records.push(DriverRecord::Start(DriverRecordStart {
                    player,
                    time: start.time,
                }));
            }
            DriverInput::Event(Event::WayPoint(way_point)) => {
                self.way_point(way_point, &mut records)
            }
            DriverInput::Event(Event::Respawn(respawn)) => {
                let player = self.number(&respawn.account_id, &mut records);
                records.push(DriverRecord::Respawn(player));
            }
            DriverInput::Event(Event::GiveUp(give_up)) => {
                let player = self.number(&give_up.account_id, &mut records);
                records.push(DriverRecord::GiveUp(player));
            }
            DriverInput::Event(event) if Driver::handles(event) => {
                records.push(DriverRecord::Event(event.clone()));
            }
            DriverInput::Event(_) => {}
            DriverInput::Playlist(maps) => records.push(DriverRecord::Playlist(maps.clone())),
            DriverInput::Players(players) => records.push(DriverRecord::Players(players.clone())),
            DriverInput::Delivered => records.push(DriverRecord::Delivered),
            DriverInput::Resumed => records.push(DriverRecord::Resumed),
            DriverInput::Settings(settings) => {
                records.push(DriverRecord::Settings(settings.clone()));
            }
            DriverInput::Teams(teams) => records.push(DriverRecord::Teams(teams.clone())),
        }
        for record in &records {
            self.replay.input(record);
        }
        records
    }

    /// The number of a player, recorded before the first record which refers to it.
    fn number(&mut self, account_id: &str, records: &mut Vec<DriverRecord>) -> u32 {
        if let Some(number) = self.numbers.get(account_id) {
            return *number;
        }
        let player = self.numbers.len() as u32;
        self.numbers.insert(account_id.into(), player);
        let record = DriverRecord::Player(DriverRecordPlayer {
            player,
            account_id: account_id.into(),
        });
        // Later records of the same input already refer to it.
        self.replay.input(&record);
        records.push(record);
        player
    }

    fn way_point(&mut self, way_point: &WayPoint, records: &mut Vec<DriverRecord>) {
        let compact = |value: u32| u16::try_from(value).ok();
        let index = compact(way_point.checkpoint_in_race);
        let lap_length = compact(way_point.checkpoint_in_lap.saturating_add(1));
        let consistent = way_point.current_race_checkpoints.last() == Some(&way_point.racetime)
            && way_point.current_race_checkpoints.len() as u64
                == u64::from(way_point.checkpoint_in_race) + 1;
        let (Some(index), Some(lap_length), true) = (index, lap_length, consistent) else {
            // Nothing compact tells this waypoint, it goes as it came.
            records.push(DriverRecord::Event(Event::WayPoint(way_point.clone())));
            return;
        };

        let player = self.number(&way_point.account_id, records);
        let (predicted, time) = self.replay.predicts(player, way_point.racetime);
        let start = way_point.time.wrapping_sub(way_point.racetime);
        if predicted == way_point.current_race_checkpoints && time.is_some() {
            if time != Some(way_point.time) {
                // The race started at another time than the records tell, e.g. its
                // start of a round before got sent but not the one of this race.
                let record = DriverRecord::Clock(DriverRecordStart {
                    player,
                    time: start,
                });
                self.replay.input(&record);
                records.push(record);
            }
        } else {
            let before = way_point.current_race_checkpoints.len() - 1;
            let record = DriverRecord::Resync(DriverRecordResync {
                player,
                start,
                checkpoints: way_point.current_race_checkpoints[..before].to_vec(),
            });
            self.replay.input(&record);
            records.push(record);
        }

        let lap_end = DriverRecordLapEnd {
            player,
            race_time: way_point.racetime,
            index,
            lap_length: if way_point.is_end_lap { lap_length } else { 0 },
        };
        records.push(if way_point.is_end_race {
            DriverRecord::Finish(lap_end)
        } else if way_point.is_end_lap {
            DriverRecord::LapEnd(lap_end)
        } else {
            DriverRecord::Checkpoint(DriverRecordCheckpoint {
                player,
                race_time: way_point.racetime,
                index,
            })
        });
    }
}

/// A driver fed with records, the same in the bridge and the manager.
pub struct RecordedDriver {
    driver: Driver,
    replayer: Replayer,
}

impl RecordedDriver {
    pub fn start(
        source: &dyn ModeSource,
        settings: &str,
        limits: DriverLimits,
    ) -> Result<RecordedDriver, DriverError> {
        Ok(RecordedDriver {
            driver: Driver::start(source, settings, limits)?,
            replayer: Replayer::default(),
        })
    }

    /// The driver of a match from its settings and every record of it, in order.
    pub fn replay<'a>(
        source: &dyn ModeSource,
        settings: &str,
        limits: DriverLimits,
        records: impl IntoIterator<Item = &'a DriverRecord>,
    ) -> Result<RecordedDriver, DriverError> {
        let mut driver = RecordedDriver::start(source, settings, limits)?;
        for record in records {
            driver.apply(record)?;
        }
        Ok(driver)
    }

    /// Applies a record. Records which are no input reply nothing.
    pub fn apply(&mut self, record: &DriverRecord) -> Result<DriverReply, DriverError> {
        match self.replayer.input(record) {
            Some(input) => self.driver.apply(&input),
            None => Ok(DriverReply::default()),
        }
    }

    pub fn driver(&self) -> &Driver {
        &self.driver
    }

    /// The account of a player number of the records.
    pub fn account(&self, player: u32) -> Option<&str> {
        self.replayer.account(player)
    }
}

/// A driver which records its inputs, for the bridge: every input goes through its
/// records, so the driver is the one the manager replays from them.
pub struct RecordingDriver {
    recorder: Recorder,
    driver: RecordedDriver,
}

impl RecordingDriver {
    pub fn start(
        source: &dyn ModeSource,
        settings: &str,
        limits: DriverLimits,
    ) -> Result<RecordingDriver, DriverError> {
        Ok(RecordingDriver {
            recorder: Recorder::default(),
            driver: RecordedDriver::start(source, settings, limits)?,
        })
    }

    /// The driver of a match from its settings and every input it got, in order,
    /// through their records.
    pub fn replay<'a>(
        source: &dyn ModeSource,
        settings: &str,
        limits: DriverLimits,
        inputs: impl IntoIterator<Item = &'a DriverInput>,
    ) -> Result<RecordingDriver, DriverError> {
        let mut driver = RecordingDriver::start(source, settings, limits)?;
        for input in inputs {
            driver.apply(input)?;
        }
        Ok(driver)
    }

    /// Applies an input through its records. Returns the records, to stream them in
    /// order, and the replies to all of them as one. A round which counted is
    /// followed by the digest of the standings.
    pub fn apply(
        &mut self,
        input: &DriverInput,
    ) -> Result<(Vec<DriverRecord>, DriverReply), DriverError> {
        let mut records = self.recorder.record(input);
        let mut reply = DriverReply::default();
        for record in &records {
            let mut next = self.driver.apply(record)?;
            reply.commands.append(&mut next.commands);
            reply.round_result = next.round_result.or(reply.round_result);
            reply.rejected = next.rejected.or(reply.rejected);
        }
        if reply.round_result.is_some() {
            records.push(DriverRecord::Digest(self.driver.driver.digest()));
        }
        Ok((records, reply))
    }

    pub fn driver(&self) -> &Driver {
        self.driver.driver()
    }

    pub fn into_driver(self) -> Driver {
        self.driver.driver
    }
}

impl Driver {
    /// What the standings are, to compare two drivers of the same records.
    pub fn digest(&self) -> DriverRecordDigest {
        let standings = serde_json::to_string(&self.standings())
            .expect("Only types json can represent are published");
        DriverRecordDigest {
            rounds: self.standings.rounds_played,
            standings: fnv(standings.as_bytes()),
        }
    }
}

/// FNV-1a, which stays the same across platforms and versions of Rust.
fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use tm_server_types::event::{Event, StartLine, WayPoint};

    use super::*;

    fn start(account_id: &str, time: u32) -> DriverInput {
        DriverInput::Event(Event::StartLine(StartLine {
            account_id: account_id.into(),
            time,
        }))
    }

    /// A waypoint of a race which started at `start`, three checkpoints per lap.
    fn pass(account_id: &str, start: u32, checkpoints: &[u32], finished: bool) -> DriverInput {
        let index = checkpoints.len() as u32 - 1;
        let racetime = *checkpoints.last().unwrap();
        DriverInput::Event(Event::WayPoint(WayPoint {
            account_id: account_id.into(),
            time: start + racetime,
            racetime,
            laptime: 0,
            speed: 87.5,
            checkpoint_in_race: index,
            checkpoint_in_lap: index % 3,
            is_end_race: finished,
            is_end_lap: finished || index % 3 == 2,
            is_infinite_laps: false,
            is_independent_laps: false,
            current_race_checkpoints: checkpoints.to_vec(),
            current_lap_checkpoints: Vec::new(),
            block_id: "block".into(),
        }))
    }

    /// What the driver reads of a start or waypoint.
    fn read(input: &DriverInput) -> String {
        match input {
            DriverInput::Event(Event::StartLine(start)) => {
                format!("start {} at {}", start.account_id, start.time)
            }
            DriverInput::Event(Event::WayPoint(w)) => {
                let lap = if w.is_end_lap {
                    format!(" lap of {}", w.checkpoint_in_lap + 1)
                } else {
                    String::new()
                };
                format!(
                    "{} at {} #{} {:?}{}{lap}",
                    w.account_id,
                    w.time,
                    w.checkpoint_in_race,
                    w.current_race_checkpoints,
                    if w.is_end_race { " finish" } else { "" },
                )
            }
            other => format!("{other:?}"),
        }
    }

    /// The inputs the records of these inputs replay as, and the records.
    fn through_records(inputs: &[DriverInput]) -> (Vec<String>, Vec<DriverRecord>) {
        let mut recorder = Recorder::default();
        let mut replayer = Replayer::default();
        let mut replayed = Vec::new();
        let mut records = Vec::new();
        for input in inputs {
            for record in recorder.record(input) {
                replayed.extend(replayer.input(&record).as_ref().map(read));
                records.push(record);
            }
        }
        (replayed, records)
    }

    #[test]
    fn races_replay_as_the_server_sent_them() {
        let inputs = [
            start("a", 1_000),
            pass("a", 1_000, &[10_000], false),
            pass("a", 1_000, &[10_000, 20_000], false),
            pass("a", 1_000, &[10_000, 20_000, 30_000], false),
            pass("a", 1_000, &[10_000, 20_000, 30_000, 40_000], true),
            // A new race of a right after.
            start("a", 42_000),
            pass("a", 42_000, &[9_000], false),
        ];

        let (replayed, records) = through_records(&inputs);

        assert_eq!(replayed, inputs.iter().map(read).collect::<Vec<_>>());
        // Only the number of the player and what is new, nothing to catch up.
        assert!(matches!(records[0], DriverRecord::Player(_)));
        assert!(matches!(records[4], DriverRecord::LapEnd(_)));
        assert!(matches!(records[5], DriverRecord::Finish(_)));
        assert_eq!(records.len(), inputs.len() + 1);
    }

    #[test]
    fn races_the_server_lost_parts_of_replay_whole() {
        let inputs = [
            start("a", 1_000),
            pass("a", 1_000, &[10_000], false),
            // The waypoint of the second checkpoint got lost.
            pass("a", 1_000, &[10_000, 20_000, 30_000], false),
            pass("a", 1_000, &[10_000, 20_000, 30_000, 40_000], true),
            // The start of the next race got lost.
            pass("b", 5_000, &[11_000], false),
            pass("a", 50_000, &[12_000], false),
        ];

        let (replayed, records) = through_records(&inputs);

        let without_lost_start: Vec<String> = inputs.iter().map(read).collect();
        assert_eq!(replayed, without_lost_start);
        let resyncs = records
            .iter()
            .filter(|r| matches!(r, DriverRecord::Resync(_)))
            .count();
        assert_eq!(resyncs, 3, "{records:#?}");
    }

    #[test]
    fn races_keep_their_server_time_when_their_start_got_lost() {
        let round_start =
            DriverInput::Event(Event::StartRoundStart(tm_server_types::event::StartRound {
                count: 2,
                time: 0,
            }));
        let inputs = [
            // Started, never passed a checkpoint, and the start of the next round got lost.
            start("a", 1_000),
            round_start.clone(),
            pass("a", 60_000, &[10_000], false),
            // Within a round: gave up without a checkpoint, the next start got lost.
            start("b", 61_000),
            pass("b", 90_000, &[10_000], false),
        ];

        let (replayed, records) = through_records(&inputs);

        let expected: Vec<String> = inputs
            .iter()
            .filter(|i| !matches!(i, DriverInput::Event(Event::StartRoundStart(_))))
            .map(read)
            .collect();
        let replayed: Vec<String> = replayed
            .into_iter()
            .filter(|r| !r.contains("StartRoundStart"))
            .collect();
        assert_eq!(replayed, expected);
        assert!(
            matches!(records[3], DriverRecord::Resync(_)),
            "{records:#?}"
        );
        assert!(matches!(records[7], DriverRecord::Clock(_)), "{records:#?}");
    }

    #[test]
    fn waypoints_nothing_compact_tells_are_recorded_as_they_came() {
        let mut odd = pass("a", 0, &[10_000, 20_000], false);
        if let DriverInput::Event(Event::WayPoint(w)) = &mut odd {
            w.checkpoint_in_race = 70_000;
        }
        let inputs = [
            start("a", 0),
            odd.clone(),
            pass("a", 0, &[10_000, 20_000, 30_000], false),
        ];

        let (replayed, records) = through_records(&inputs);

        assert!(matches!(
            records[2],
            DriverRecord::Event(Event::WayPoint(_))
        ));
        assert_eq!(replayed, inputs.iter().map(read).collect::<Vec<_>>());
    }

    #[test]
    fn only_callbacks_the_driver_handles_and_race_statistics_are_recorded() {
        let mut recorder = Recorder::default();
        let respawn = DriverInput::Event(Event::Respawn(tm_server_types::event::Respawn {
            account_id: "a".into(),
            time: 0,
            number_respawns: 1,
            racetime: 0,
            laptime: 0,
            checkpoint_in_race: 0,
            checkpoint_in_lap: 0,
            speed: 0.,
        }));
        let scores = DriverInput::Event(Event::WarmupStartRound(
            tm_server_types::event::WarmupRound {
                current: 1,
                total: 2,
            },
        ));

        let recorded = recorder.record(&respawn);
        assert!(matches!(
            recorded[..],
            [DriverRecord::Player(_), DriverRecord::Respawn(0)]
        ));
        assert!(recorder.record(&scores).is_empty());
        assert!(matches!(
            recorder.record(&DriverInput::Resumed)[..],
            [DriverRecord::Resumed]
        ));

        // Statistics never reach the mode.
        let mut replayer = Replayer::default();
        for record in &recorded {
            assert!(replayer.input(record).is_none());
        }
    }
}
