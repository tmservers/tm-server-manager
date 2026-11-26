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
    event::{DriverRecord, DriverTeam},
    method::{DriverEntry, DriverLabelTone, DriverStatus},
};

use crate::{
    authorization::Authorization,
    competition::node::NodeHandle,
    maps::TmMapRead,
    raw_server::occupation::TabRawServerOccupationRead,
    registration::team::TeamRead,
    tm_match::entrant::MatchEntrantRead,
    tm_match::{
        MatchRead,
        leaderboard::{MatchLeaderboardWrite, MatchRoundPlayer},
        state::MatchStateRead,
    },
    user::{UserRead, UserWrite},
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

/// A team in the standings of a driver match of teams, as its mode left them.
#[table(accessor = tab_match_team_standing, vis_private)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchTeamStanding {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(hash)]
    pub match_id: u32,
    pub team_id: u32,
    /// From 1, 0 if the mode did not rank the team.
    pub position: u32,
    pub score: i32,
    pub map_score: i32,
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
            map,
            result,
        } => {
            // The map the records tell, the state may already be on the next one.
            let map_id = map
                .and_then(|uid| ctx.tm_map_find_uid(&uid))
                .map(|map| map.id)
                .or_else(|| ctx.match_state_find(match_id).map(|s| s.get_map()))
                .unwrap_or(0);
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
        Effect::TeamStandings(teams) => {
            let standings = ctx.db.tab_match_team_standing();
            let stored: Vec<MatchTeamStanding> = standings.match_id().filter(match_id).collect();
            for old in stored {
                standings.id().delete(old.id);
            }
            for team in teams {
                standings.insert(MatchTeamStanding {
                    id: 0,
                    match_id,
                    team_id: team.team_id,
                    position: team.position,
                    score: team.score,
                    map_score: team.map_score,
                    label: team.label,
                    label_tone: team.label_tone,
                });
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
    /// The teams of a driver match of teams, by position and unranked teams last.
    fn match_driver_team_standings(&self, match_id: u32) -> Vec<MatchTeamStanding>;
    /// The teams among the entrants of a match, for its driver.
    fn match_driver_teams(&self, match_id: u32) -> Vec<DriverTeam>;
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

    fn match_driver_team_standings(&self, match_id: u32) -> Vec<MatchTeamStanding> {
        let mut standings: Vec<MatchTeamStanding> = self
            .db_read_only()
            .tab_match_team_standing()
            .match_id()
            .filter(match_id)
            .collect();
        standings.sort_by_key(|s| (s.position == 0, s.position, s.team_id));
        standings
    }

    fn match_driver_teams(&self, match_id: u32) -> Vec<DriverTeam> {
        self.match_entrants(match_id)
            .into_iter()
            .filter(|entrant| entrant.user_id == 0)
            .map(|entrant| DriverTeam {
                team_id: entrant.team_id,
                name: self.team_name(entrant.team_id).unwrap_or_default(),
                short_name: String::new(),
                logo_url: String::new(),
                flag_url: String::new(),
                players: self
                    .participant_users(entrant.participant())
                    .into_iter()
                    .map(|user_id| self.user_account_from_id(user_id).to_string())
                    .collect(),
            })
            .collect()
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
        self.db()
            .tab_match_team_standing()
            .match_id()
            .delete(match_id);
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

/// The teams of a driver match of teams for the members of its competition and its viewers.
#[view(accessor = match_driver_team_standings, public)]
fn match_driver_team_standings(ctx: &ViewContext, match_id: u32) -> Vec<MatchTeamStanding> {
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
    ctx.match_driver_team_standings(match_id)
}

/// The teams of the match of the server, for its bridge: the driver starts with them.
#[view(accessor = my_match_teams, public)]
fn my_match_teams(ctx: &ViewContext) -> Vec<DriverTeam> {
    let Ok(server_id) = ctx.server_id() else {
        return Vec::new();
    };
    let Some(NodeHandle::MatchV1(match_id)) = ctx.raw_server_occupation(server_id) else {
        return Vec::new();
    };
    ctx.match_driver_teams(match_id)
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

#[cfg(test)]
mod tests {
    use tm_mode_driver::DriverInput;
    use tm_mode_driver_sim::events::{InputLog, Race, at, way_point_at};
    use tm_server_types::{
        config::BuiltInMode,
        event::{DriverRecord, DriverRecordCheckpoint, Event, GiveUp, Respawn},
    };

    use super::*;
    use crate::{test_world::*, tm_match::MatchStatus};

    const ROUNDS: BuiltInMode = BuiltInMode::RoundsV1;
    const SETTINGS: &str = r#"{"points_repartition": [10, 6, 4], "rounds_per_map": 5}"#;

    fn players() -> (String, String, String) {
        (player(1), player(2), player(3))
    }

    /// Two rounds of three players, the second one without c, after a round which
    /// did not count.
    fn two_rounds() -> InputLog {
        let (a, b, c) = players();
        let mut log = InputLog::new(SETTINGS);
        log.players(&[&a, &b, &c]).match_start().map_start("m1");
        log.forced_round(&[(&c, Race::finish(&[10_000, 20_000]))]);
        log.round(&[
            (&a, Race::finish(&[10_000, 30_000])),
            (&b, Race::finish(&[10_000, 31_000])),
            (&c, Race::dnf(&[11_000])),
        ]);
        log.round(&[
            (&b, Race::finish(&[10_000, 29_000])),
            (&a, Race::finish(&[10_000, 32_000])),
        ]);
        log
    }

    /// The stored standings by account, position and score.
    fn stored(world: &World, match_id: u32) -> Vec<(String, u32, i32)> {
        world
            .test
            .anonymous_view_context()
            .match_driver_standings(match_id)
            .into_iter()
            .map(|s| (world.account(s.user_id), s.position, s.score))
            .collect()
    }

    /// The standings of the driver of the bridge, the same way.
    fn of_bridge(bridge: &Bridge) -> Vec<(String, u32, i32)> {
        let mut entries: Vec<(String, u32, i32)> = bridge
            .driver
            .driver()
            .standings()
            .entries
            .into_iter()
            .map(|e| (e.account_id, e.position, e.score))
            .collect();
        entries.sort_by_key(|(account, position, _)| (*position == 0, *position, account.clone()));
        entries
    }

    fn state(world: &World, match_id: u32) -> MatchDriverState {
        world
            .test
            .anonymous_view_context()
            .match_driver(match_id)
            .expect("the match plays the driver")
            .state
    }

    fn records(world: &World, match_id: u32) -> usize {
        world
            .test
            .anonymous_view_context()
            .db
            .tab_match_driver_record()
            .match_seq()
            .filter(match_id)
            .count()
    }

    #[test]
    fn the_manager_keeps_the_scores_the_driver_of_the_bridge_computes() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
        let mut bridge = Bridge::new(ROUNDS, SETTINGS, 1);

        bridge.play(&world, &server, &two_rounds().inputs);

        let (a, b, c) = players();
        // b wins the tie with the better last round.
        let expected = vec![(b.clone(), 1, 16), (a.clone(), 2, 16), (c.clone(), 3, 0)];
        assert_eq!(stored(&world, match_id), expected);
        assert_eq!(of_bridge(&bridge), expected);
        assert_eq!(
            world.rounds(match_id),
            vec![
                (1, a.clone(), 10, 1),
                (1, b.clone(), 6, 2),
                (1, c.clone(), 0, 3),
                (2, a.clone(), 6, 2),
                (2, b.clone(), 10, 1),
            ]
        );
        // Leaderboards rank by the stored standings.
        let ranked: Vec<(String, u16, i32)> = world
            .standings(match_id)
            .iter()
            .map(|e| (world.account(e.user_id), e.position, e.score))
            .collect();
        assert_eq!(ranked, vec![(b, 1, 16), (a, 2, 16), (c, 3, 0)]);
        assert_eq!(state(&world, match_id), MatchDriverState::Running);
    }

    /// The rounds belong to the driver, a pause or a lost bridge does not drop any of them.
    #[test]
    fn an_interruption_keeps_the_rounds_the_driver_counted() {
        for disconnect in [false, true] {
            let world = World::new();
            let server = world.server(1);
            let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
            let mut bridge = Bridge::new(ROUNDS, SETTINGS, 1);
            bridge.play(&world, &server, &two_rounds().inputs);
            let counted = world.rounds(match_id);
            assert_eq!(counted.len(), 5);

            // The server is in its second round while the records of the driver came in.
            server.post(
                &world,
                [start_match(), start_round(1), end_round(), start_round(2)],
            );
            if disconnect {
                server.disconnect(&world);
            } else {
                server.post(&world, [pause(true)]);
            }

            assert_eq!(
                world.rounds(match_id),
                counted,
                "disconnect: {disconnect}, the counted rounds stay"
            );
        }
    }

    #[test]
    fn a_record_sent_again_changes_nothing_and_one_which_skips_records_is_refused() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
        let mut bridge = Bridge::new(ROUNDS, SETTINGS, 1);
        bridge.play(&world, &server, &two_rounds().inputs);
        let before = stored(&world, match_id);
        let count = records(&world, match_id);

        // The bridge lost the answers and sends the last two rounds' records again.
        for (index, record) in bridge.records.iter().enumerate().skip(10) {
            server
                .stream(&world, 1, index as u32 + 1, record.clone())
                .expect("a record sent again is taken");
        }
        assert_eq!(records(&world, match_id), count);
        assert_eq!(stored(&world, match_id), before);

        let next = count as u32 + 2;
        let skipped = server.stream(&world, 1, next, DriverRecord::Delivered);
        assert!(skipped.is_err_and(|e| e.contains("skips records")));
        assert_eq!(records(&world, match_id), count);
    }

    #[test]
    fn the_match_is_replayed_from_its_log_when_its_replay_is_gone() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
        let mut bridge = Bridge::new(ROUNDS, SETTINGS, 1);
        let log = two_rounds();
        let half = log.inputs.len() / 2;

        bridge.play(&world, &server, &log.inputs[..half]);
        // E.g. the module restarted.
        EMULATIONS.with(|emulations| emulations.borrow_mut().clear());
        bridge.play(&world, &server, &log.inputs[half..]);

        assert_eq!(stored(&world, match_id), of_bridge(&bridge));
        assert_eq!(world.rounds(match_id).len(), 5);
        assert_eq!(state(&world, match_id), MatchDriverState::Running);
    }

    #[test]
    fn other_standings_of_the_bridge_flag_the_match_and_keep_the_scores_of_the_manager() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
        let mut bridge = Bridge::new(ROUNDS, SETTINGS, 1);
        let mut records = bridge.record(&two_rounds().inputs);
        // The bridge played another version of the mode in the first round.
        let first = records
            .iter_mut()
            .find_map(|record| match record {
                DriverRecord::Digest(digest) => Some(digest),
                _ => None,
            })
            .expect("a round counted");
        first.standings ^= 1;

        for (index, record) in records.into_iter().enumerate() {
            server
                .stream(&world, 1, index as u32 + 1, record)
                .expect("the record is taken");
        }

        assert_eq!(state(&world, match_id), MatchDriverState::Diverged(1));
        assert_eq!(stored(&world, match_id), of_bridge(&bridge));
    }

    #[test]
    fn races_are_kept_by_lap_with_their_respawns_and_give_ups() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
        let mut bridge = Bridge::new(ROUNDS, SETTINGS, 1);
        let (a, b, _) = players();
        let mut log = InputLog::new(SETTINGS);
        log.players(&[&a, &b])
            .match_start()
            .map_start("m1")
            .round_start();
        // Two laps of two checkpoints each.
        log.races(&[(
            &a,
            Race::finish(&[10_000, 20_000, 30_000, 40_000]).in_laps(2),
        )]);
        let respawn = Event::Respawn(Respawn {
            account_id: b.clone(),
            time: 0,
            number_respawns: 1,
            racetime: 5_000,
            laptime: 5_000,
            checkpoint_in_race: 0,
            checkpoint_in_lap: 0,
            speed: 0.,
        });
        log.event(tm_mode_driver_sim::events::start_line(&b));
        log.event(at(way_point_at(&b, &[12_000], false), 20_000));
        log.event(respawn.clone()).event(respawn);
        log.event(Event::GiveUp(GiveUp {
            account_id: b.clone(),
            time: 25_000,
        }));
        log.push(DriverInput::Event(tm_mode_driver_sim::events::hold(
            3,
            tm_server_types::event::DriverSection::RoundEnd,
        )));

        bridge.play(&world, &server, &log.inputs);

        let mut laps: Vec<(String, u16, u16, u16, Vec<u32>, bool, u16, bool)> = world
            .test
            .anonymous_view_context()
            .db
            .tab_match_race_lap()
            .match_id()
            .filter(match_id)
            .map(|l| {
                (
                    world.account(l.user_id),
                    l.round,
                    l.race,
                    l.lap,
                    l.checkpoints,
                    l.finished,
                    l.respawns,
                    l.gave_up,
                )
            })
            .collect();
        laps.sort();
        assert_eq!(
            laps,
            vec![
                (a.clone(), 1, 0, 0, vec![10_000, 20_000], false, 0, false),
                (a.clone(), 1, 0, 1, vec![30_000, 40_000], true, 0, false),
                (b.clone(), 1, 0, 0, vec![12_000], false, 2, true),
            ]
        );
    }

    #[test]
    fn a_new_run_of_the_driver_starts_the_scores_over() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
        let mut first = Bridge::new(ROUNDS, SETTINGS, 1);
        first.play(&world, &server, &two_rounds().inputs);

        // A new run has to start at its first record.
        let late = server.stream(&world, 2, 5, DriverRecord::Delivered);
        assert!(late.is_err_and(|e| e.contains("starts with record 1")));
        assert_eq!(stored(&world, match_id).len(), 3);

        let (a, b, _) = players();
        let mut log = InputLog::new(SETTINGS);
        log.players(&[&a, &b]).match_start().map_start("m1");
        log.round(&[(&a, Race::finish(&[30_000])), (&b, Race::finish(&[31_000]))]);
        let mut second = Bridge::new(ROUNDS, SETTINGS, 2);
        second.play(&world, &server, &log.inputs);

        assert_eq!(
            stored(&world, match_id),
            vec![(a.clone(), 1, 10), (b.clone(), 2, 6)]
        );
        assert_eq!(world.rounds(match_id), vec![(1, a, 10, 1), (1, b, 6, 2)]);
        assert_eq!(records(&world, match_id), second.records.len());
    }

    #[test]
    fn records_only_count_while_the_match_runs() {
        let world = World::new();
        let server = world.server(1);
        server.report_modes(&world, &["rounds-v1"]);
        let match_id = world.configured_match_with("Final", driver_config_with(ROUNDS, SETTINGS));
        world.as_owner(|ctx| crate::tm_match::match_set_preparation(ctx, match_id));

        let early = server.stream(&world, 1, 1, DriverRecord::Delivered);

        assert!(early.is_err_and(|e| e.contains("Preparation")));
        assert_eq!(records(&world, match_id), 0);
    }

    #[test]
    fn a_match_with_settings_its_mode_rejects_takes_no_server() {
        let world = World::new();
        let server = world.server(1);
        server.report_modes(&world, &["rounds-v1"]);
        let config = driver_config_with(ROUNDS, r#"{"rounds_per_map": "five"}"#);
        let match_id = world.configured_match_with("Final", config);

        let refused =
            world.try_as_owner(|ctx| crate::tm_match::match_set_preparation(ctx, match_id));

        let refused = refused.unwrap_err();
        assert!(
            refused.contains("Invalid settings") && refused.contains("five"),
            "{refused}"
        );
        assert_eq!(world.match_status(match_id), MatchStatus::Configured);
        assert_eq!(world.server_of(NodeHandle::MatchV1(match_id)), None);
    }

    #[test]
    fn driver_matches_ignore_the_races_of_the_event_stream() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.live_driver_match(&server, ROUNDS, SETTINGS);
        let a = player(1);

        server.post(
            &world,
            [
                start_match(),
                start_round(1),
                start_line(&a),
                finish(&a, 30_000),
            ],
        );

        assert_eq!(world.rounds(match_id), vec![]);
    }

    #[test]
    fn a_match_of_teams_keeps_the_standings_of_its_teams() {
        let world = World::new();
        let server = world.server(1);
        let tmwt = BuiltInMode::Tmwt2v2V1;
        let match_id = world.live_driver_match(&server, tmwt, "{}");
        let (a, b, c) = players();
        let d = player(4);
        let team =
            |team_id, name: &str, players: [&String; 2]| tm_server_types::event::DriverTeam {
                team_id,
                name: name.into(),
                short_name: String::new(),
                logo_url: String::new(),
                flag_url: String::new(),
                players: players.iter().map(|p| p.to_string()).collect(),
            };
        let mut log = InputLog::new("{}");
        log.players(&[&a, &b, &c, &d]);
        log.push(DriverInput::Teams(vec![
            team(7, "Red", [&a, &b]),
            team(3, "Blue", [&c, &d]),
        ]));
        log.match_start().map_start("m1");
        // Red finishes first and second: an ace.
        log.round(&[
            (&a, Race::finish(&[30_000])),
            (&b, Race::finish(&[31_000])),
            (&c, Race::finish(&[32_000])),
        ]);
        let mut bridge = Bridge::new(tmwt, "{}", 1);

        bridge.play(&world, &server, &log.inputs);

        let teams: Vec<(u32, u32, i32, i32)> = world
            .test
            .anonymous_view_context()
            .match_driver_team_standings(match_id)
            .into_iter()
            .map(|t| (t.team_id, t.position, t.score, t.map_score))
            .collect();
        assert_eq!(teams, vec![(7, 1, 0, 3), (3, 2, 0, 0)]);
        assert_eq!(stored(&world, match_id)[0], (a.clone(), 1, 3));
        assert_eq!(state(&world, match_id), MatchDriverState::Running);
    }

    /// Checkpoints are most of a match, the log of a long race stays small.
    #[test]
    fn a_checkpoint_is_eleven_bytes() {
        let checkpoint = DriverRecord::Checkpoint(DriverRecordCheckpoint {
            player: 3,
            race_time: 61_234,
            index: 17,
        });
        assert_eq!(
            spacetimedb::sats::bsatn::to_vec(&checkpoint).unwrap().len(),
            11
        );
    }

    #[test]
    fn a_round_keeps_the_map_its_records_were_played_on() {
        use crate::{maps::TmMapRead, tm_match::leaderboard::MatchLeadearboardRead};

        let world = World::new();
        let server = world.server(1);
        let settings = r#"{"points_repartition": [10, 6, 4], "rounds_per_map": 1}"#;
        let match_id = world.live_driver_match(&server, ROUNDS, settings);
        // The server already started both maps before the records arrive, its state is on the second one.
        let started = |uid: &str| {
            let mut event = tm_mode_driver_sim::events::map_start(uid);
            if let Event::StartMapStart(start) = &mut event {
                start.map.author_account_id = player(9);
            }
            event
        };
        server.post(&world, [started("m1"), started("m2")]);
        let (a, b, _) = players();
        let mut log = InputLog::new(settings);
        log.players(&[&a, &b]).match_start().map_start("m1");
        log.round(&[(&a, Race::finish(&[30_000])), (&b, Race::finish(&[31_000]))]);
        log.map_end().map_start("m2");
        log.round(&[(&b, Race::finish(&[30_000])), (&a, Race::finish(&[31_000]))]);
        let mut bridge = Bridge::new(ROUNDS, settings, 1);

        bridge.play(&world, &server, &log.inputs);

        let cx = world.test.anonymous_view_context();
        let map = |uid: &str| {
            cx.tm_map_find_uid(&uid.to_string())
                .expect("the map is known")
                .id
        };
        let mut maps: Vec<(u16, u32)> = cx
            .match_rounds(match_id)
            .iter()
            .map(|e| (e.round, e.map_id))
            .collect();
        maps.dedup();
        assert_eq!(maps, vec![(1, map("m1")), (2, map("m2"))]);
    }
}
