//! Matches which play a mode of the driver.
//!
//! The bridge runs the driver next to the server and decides, without waiting for the
//! database. It streams every record of the driver's inputs here, one by one and
//! numbered. The manager keeps them as the log of the match and replays them with the
//! same driver, which gives the same standings, to store the scores of the match:
//! its standings, the rounds which counted and the laps of every race.
//!
//! The replay of a match is kept in memory between records. When it is not the one of
//! the last stored record, e.g. after the module restarted or a transaction failed, it
//! is replayed from the log again.

mod emulation;

use std::{cell::RefCell, collections::BTreeMap};

use spacetimedb::{
    CtxDbRead, CtxDbWrite, ReducerContext, SpacetimeType, Table, Uuid, ViewContext, reducer, table,
    view,
};
use tm_server_types::{
    config::DriverMode,
    event::DriverRecord,
    method::{DriverEntry, DriverLabelTone, DriverStatus},
};

use crate::{
    authorization::Authorization,
    competition::node::NodeHandle,
    raw_server::occupation::TabRawServerOccupationRead,
    tm_match::{
        MatchRead,
        leaderboard::{MatchLeaderboardWrite, MatchRoundPlayer},
        state::MatchStateRead,
    },
    user::UserWrite,
};

use emulation::{Effect, Emulation};

/// The mode a match plays with the driver and how far its log is.
#[table(accessor = tab_match_driver, vis_private)]
#[derive(Debug, Clone)]
pub(crate) struct MatchDriver {
    #[primary_key]
    pub(crate) match_id: u32,
    pub(crate) mode: DriverMode,
    /// The json settings the match started with.
    pub(crate) settings: String,
    /// The run of the driver of the bridge the log is of. A driver which starts over,
    /// e.g. when the match goes live, numbers its records from 1 again.
    pub(crate) session: u32,
    /// The record stored last, 0 before the first.
    pub(crate) last_seq: u32,
    pub(crate) state: MatchDriverState,
}

#[derive(Debug, Clone, PartialEq, Eq, SpacetimeType)]
pub enum MatchDriverState {
    Running,
    /// The standings of the bridge differed from the ones of the manager after this many
    /// rounds counted. The manager keeps the scores it computes, an admin has to check.
    Diverged(u32),
    /// The mode failed, the manager stopped replaying the match.
    Faulted(String),
}

/// The log of a match: every record of the inputs of its driver in order.
#[table(accessor = tab_match_driver_record, vis_private,
    index(accessor = match_seq, btree(columns = [match_id, seq])))]
pub(crate) struct MatchDriverRecord {
    #[primary_key]
    #[auto_inc]
    id: u64,
    match_id: u32,
    seq: u32,
    pub(crate) record: DriverRecord,
}

/// A player in the standings of a driver match, as its mode left them.
#[table(accessor = tab_match_standing, vis_private,
    index(accessor = match_user, btree(columns = [match_id, user_id])))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchStanding {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(hash)]
    pub match_id: u32,
    pub user_id: u32,
    /// From 1, 0 if the mode did not rank the player.
    pub position: u32,
    pub score: i32,
    /// E.g. the time of the best race, -1 without one.
    pub time: i32,
    pub status: DriverStatus,
    pub label: String,
    pub label_tone: DriverLabelTone,
}

/// A lap of a race in a driver match. A race is split into its laps, so a checkpoint
/// only changes a small row, even in a long endurance race.
#[table(accessor = tab_match_race_lap, vis_private,
    index(accessor = racer, btree(columns = [match_id, round, user_id])))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchRaceLap {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(hash)]
    pub match_id: u32,
    /// The round the race was in, from 1, warmups excluded. Rounds which do not count, e.g.
    /// interrupted ones, have their number too, so it is not the round of the standings.
    pub round: u16,
    pub user_id: u32,
    /// Of the player in the round, from 0, e.g. the restarts of time attack.
    pub race: u16,
    /// Of the race, from 0.
    pub lap: u16,
    /// Race times of the checkpoints in this lap, the line that ends it included.
    pub checkpoints: Vec<u32>,
    /// The race ended at the finish in this lap.
    pub finished: bool,
    pub respawns: u16,
    pub gave_up: bool,
}

thread_local! {
    /// The replay of every match after the record it got last: session and seq.
    static EMULATIONS: RefCell<BTreeMap<u32, (u32, u32, Emulation)>> = RefCell::default();
}

/// The bridge of the server streams the next record of the driver of its match.
/// A record it sent before is accepted again and changes nothing, one which skips
/// records is refused.
#[reducer]
pub(crate) fn match_driver_record(
    ctx: &ReducerContext,
    session: u32,
    seq: u32,
    record: DriverRecord,
) -> Result<(), String> {
    let server_id = ctx.server_id()?;
    let Some(NodeHandle::MatchV1(match_id)) = ctx.raw_server_occupation(server_id) else {
        return Err("The server plays no match.".into());
    };
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Err("The match of the server does not exist.".into());
    };
    if !tm_match.is_live() && !tm_match.is_recovery() {
        return Err(format!(
            "The match is {:?}, records only count while it runs.",
            tm_match.status()
        ));
    }
    let Some(mut driver) = ctx.db.tab_match_driver().match_id().find(match_id) else {
        return Err("The match plays no mode of the driver.".into());
    };

    if session != driver.session {
        if seq != 1 {
            return Err(format!(
                "A new run of the driver starts with record 1, not {seq}."
            ));
        }
        // The driver of the bridge started over, so does the match.
        ctx.match_driver_clear(match_id);
        driver.session = session;
        driver.last_seq = 0;
        driver.state = MatchDriverState::Running;
    }
    if seq <= driver.last_seq {
        return Ok(());
    }
    if seq != driver.last_seq + 1 {
        return Err(format!(
            "Record {seq} skips records, the next one is {}.",
            driver.last_seq + 1
        ));
    }

    ctx.db.tab_match_driver_record().insert(MatchDriverRecord {
        id: 0,
        match_id,
        seq,
        record: record.clone(),
    });
    driver.last_seq = seq;

    if driver.state != MatchDriverState::Running
        && !matches!(driver.state, MatchDriverState::Diverged(_))
    {
        ctx.db.tab_match_driver().match_id().update(driver);
        return Ok(());
    }

    match emulate(ctx, &driver, &record) {
        Ok(effects) => {
            for effect in effects {
                apply_effect(ctx, &mut driver, effect)?;
            }
        }
        Err(error) => {
            log::error!("The driver of match {match_id} failed: {error}");
            driver.state = MatchDriverState::Faulted(error.to_string());
        }
    }
    ctx.db.tab_match_driver().match_id().update(driver);
    Ok(())
}

/// Applies the record to the replay of the match, which is replayed from the log first
/// if the one in memory is not after the record before.
fn emulate(
    ctx: &ReducerContext,
    driver: &MatchDriver,
    record: &DriverRecord,
) -> Result<Vec<Effect>, tm_mode_driver::DriverError> {
    let cached = EMULATIONS.with(|emulations| emulations.borrow_mut().remove(&driver.match_id));
    let mut emulation = match cached {
        Some((session, seq, emulation))
            if session == driver.session && seq + 1 == driver.last_seq =>
        {
            emulation
        }
        _ => replay(ctx, driver)?,
    };
    let effects = emulation.apply(record)?;
    EMULATIONS.with(|emulations| {
        emulations.borrow_mut().insert(
            driver.match_id,
            (driver.session, driver.last_seq, emulation),
        )
    });
    Ok(effects)
}

/// The replay of the match after every stored record but the last.
fn replay(
    ctx: &impl CtxDbRead,
    driver: &MatchDriver,
) -> Result<Emulation, tm_mode_driver::DriverError> {
    let mut emulation = Emulation::start(driver.mode, &driver.settings)?;
    let before = driver.last_seq.saturating_sub(1);
    for stored in ctx
        .db_read_only()
        .tab_match_driver_record()
        .match_seq()
        .filter((driver.match_id, 1..=before))
    {
        emulation.apply(&stored.record)?;
    }
    Ok(emulation)
}

fn apply_effect(
    ctx: &ReducerContext,
    driver: &mut MatchDriver,
    effect: Effect,
) -> Result<(), String> {
    let match_id = driver.match_id;
    match effect {
        Effect::Lap { player, round, lap } => {
            let Some(user_id) = player_user(ctx, match_id, player)? else {
                return Ok(());
            };
            let laps = ctx.db.tab_match_race_lap();
            let stored = laps
                .racer()
                .filter((match_id, round, user_id))
                .find(|row| row.race == lap.race && row.lap == lap.lap);
            let row = MatchRaceLap {
                id: stored.as_ref().map_or(0, |row| row.id),
                match_id,
                round,
                user_id,
                race: lap.race,
                lap: lap.lap,
                checkpoints: lap.checkpoints,
                finished: lap.finished,
                respawns: lap.respawns,
                gave_up: lap.gave_up,
            };
            if stored.is_some() {
                laps.id().update(row);
            } else {
                laps.insert(row);
            }
        }
        Effect::Standings(entries) => {
            let standings = ctx.db.tab_match_standing();
            let mut stored: BTreeMap<u32, MatchStanding> = standings
                .match_id()
                .filter(match_id)
                .map(|row| (row.user_id, row))
                .collect();
            for entry in entries {
                let Some(user_id) = account_user(ctx, &entry.account_id)? else {
                    continue;
                };
                let old = stored.remove(&user_id);
                let row = standing(match_id, user_id, old.as_ref().map_or(0, |o| o.id), &entry);
                match old {
                    Some(old) if old == row => {}
                    Some(_) => _ = standings.id().update(row),
                    None => _ = standings.insert(row),
                }
            }
            for (_, gone) in stored {
                standings.id().delete(gone.id);
            }
        }
        Effect::Round {
            number,
            round,
            result,
        } => {
            let map_id = ctx.match_state_find(match_id).map_or(0, |s| s.get_map());
            let round_number = u16::try_from(number).unwrap_or(u16::MAX);
            for player in result.results {
                let Some(user_id) = account_user(ctx, &player.account_id)? else {
                    continue;
                };
                let row = MatchRoundPlayer::of_driver(
                    match_id,
                    user_id,
                    round_number,
                    player.points,
                    u16::try_from(player.position).unwrap_or(u16::MAX),
                    player.time,
                    map_id,
                    round,
                );
                ctx.match_round_player_insert(row)?;
            }
        }
        Effect::Diverged(theirs) => {
            if driver.state == MatchDriverState::Running {
                log::error!(
                    "Match {match_id} diverged after {} rounds: the bridge has other standings.",
                    theirs.rounds
                );
                driver.state = MatchDriverState::Diverged(theirs.rounds);
            }
        }
    }
    Ok(())
}

fn standing(match_id: u32, user_id: u32, id: u64, entry: &DriverEntry) -> MatchStanding {
    MatchStanding {
        id,
        match_id,
        user_id,
        position: entry.position,
        score: entry.score,
        time: entry.time,
        status: entry.status,
        label: entry.label.clone(),
        label_tone: entry.label_tone,
    }
}

/// The user of a player number of the records of the match.
fn player_user(ctx: &ReducerContext, match_id: u32, player: u32) -> Result<Option<u32>, String> {
    let account = EMULATIONS.with(|emulations| {
        emulations
            .borrow()
            .get(&match_id)
            .and_then(|(_, _, emulation)| emulation.account(player).map(String::from))
    });
    match account {
        Some(account) => account_user(ctx, &account),
        None => Ok(None),
    }
}

/// The user of an account, created for players the manager has not seen yet.
fn account_user(ctx: &ReducerContext, account_id: &str) -> Result<Option<u32>, String> {
    match Uuid::parse_str(account_id) {
        Ok(account_id) => ctx.user_id_or_insert(account_id).map(Some),
        Err(_) => {
            log::warn!("{account_id} is no account, it is left out of the scores.");
            Ok(None)
        }
    }
}

pub(crate) trait MatchDriverRead {
    fn match_driver(&self, match_id: u32) -> Option<MatchDriver>;
    /// The standings of a driver match, by position and unranked players last.
    fn match_driver_standings(&self, match_id: u32) -> Vec<MatchStanding>;
}

impl<Db: CtxDbRead> MatchDriverRead for Db {
    fn match_driver(&self, match_id: u32) -> Option<MatchDriver> {
        self.db_read_only()
            .tab_match_driver()
            .match_id()
            .find(match_id)
    }

    fn match_driver_standings(&self, match_id: u32) -> Vec<MatchStanding> {
        let mut standings: Vec<MatchStanding> = self
            .db_read_only()
            .tab_match_standing()
            .match_id()
            .filter(match_id)
            .collect();
        standings.sort_by_key(|s| (s.position == 0, s.position, s.user_id));
        standings
    }
}

/// Whether the mode plays with the settings, it starts once to check them.
pub(crate) fn check_settings(mode: DriverMode, settings: &str) -> Result<(), String> {
    Emulation::start(mode, settings)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(crate) trait MatchDriverWrite {
    /// A driver match is prepared, its log is empty. The settings are checked before.
    fn match_driver_prepare(&self, match_id: u32, mode: DriverMode, settings: &str);
    /// Forgets the log and the scores of the driver of the match.
    fn match_driver_clear(&self, match_id: u32);
}

impl<Db: CtxDbWrite> MatchDriverWrite for Db {
    fn match_driver_prepare(&self, match_id: u32, mode: DriverMode, settings: &str) {
        self.match_driver_clear(match_id);
        self.db().tab_match_driver().match_id().delete(match_id);
        self.db().tab_match_driver().insert(MatchDriver {
            match_id,
            mode,
            settings: settings.into(),
            session: 0,
            last_seq: 0,
            state: MatchDriverState::Running,
        });
    }

    fn match_driver_clear(&self, match_id: u32) {
        EMULATIONS.with(|emulations| emulations.borrow_mut().remove(&match_id));
        let records: Vec<u64> = self
            .db()
            .tab_match_driver_record()
            .match_seq()
            .filter(match_id)
            .map(|r| r.id)
            .collect();
        for id in records {
            self.db().tab_match_driver_record().id().delete(id);
        }
        self.db().tab_match_standing().match_id().delete(match_id);
        self.db().tab_match_race_lap().match_id().delete(match_id);
        self.match_rounds_delete(match_id);
    }
}

/// The standings of a driver match for the members of its competition and its viewers.
#[view(accessor = match_driver_standings, public)]
fn match_driver_standings(ctx: &ViewContext, match_id: u32) -> Vec<MatchStanding> {
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Vec::new();
    };
    if ctx
        .auth_builder(tm_match.get_comp_id())
        .authorize()
        .is_err()
    {
        return Vec::new();
    }
    ctx.match_driver_standings(match_id)
}

/// The laps of every race of a driver match.
#[view(accessor = match_race_laps, public)]
fn match_race_laps(ctx: &ViewContext, match_id: u32) -> Vec<MatchRaceLap> {
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Vec::new();
    };
    if ctx
        .auth_builder(tm_match.get_comp_id())
        .authorize()
        .is_err()
    {
        return Vec::new();
    }
    ctx.db
        .tab_match_race_lap()
        .match_id()
        .filter(match_id)
        .collect()
}

/// How far the log of the driver of the match of the server is, for its bridge: it
/// sends the records after it again, e.g. after it lost its connection.
#[view(accessor = my_match_driver, public)]
fn my_match_driver(ctx: &ViewContext) -> Option<MatchDriver> {
    let server_id = ctx.server_id().ok()?;
    let Some(NodeHandle::MatchV1(match_id)) = ctx.raw_server_occupation(server_id) else {
        return None;
    };
    ctx.match_driver(match_id)
}

