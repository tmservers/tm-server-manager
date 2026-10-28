//! What has to hold after every input: for the driver and every mode, and for
//! each mode on its own.
//!
//! The checks of a mode test properties of its rules against the races the
//! simulation knows, e.g. that the players who lost a life in a knockout round are
//! the last ones of its race. They do not compute the standings a second way.

mod knockout;
mod reverse_cup;
mod rounds;
mod time_attack;

use std::collections::{BTreeMap, BTreeSet};

use tm_server_types::{
    event::{DriverSection, Event},
    method::{DriverCommand, DriverRound, DriverStandings, DriverStatus, DriverUsers},
};
use tm_wasm_driver::{Driver, DriverInput, DriverReply};

use crate::{
    Mode, World,
    server::{RoundTruth, Server},
};

/// A round that counted, as the checks of a mode see it.
pub(crate) struct RoundEnd<'a> {
    /// The standings before the round ended.
    pub before: &'a DriverStandings,
    pub after: &'a DriverStandings,
    pub round: &'a DriverRound,
    pub commands: &'a [DriverCommand],
    pub truth: &'a RoundTruth,
    /// The match was ended before this round.
    pub ended: bool,
}

/// A map that ends.
pub(crate) struct MapEnd<'a> {
    pub after: &'a DriverStandings,
    pub commands: &'a [DriverCommand],
    /// Maps in the playlist.
    pub playlist: u32,
}

pub(crate) trait ModeChecks {
    fn match_start(&mut self) {}
    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String>;
    fn map_end(&mut self, _end: &MapEnd) -> Result<(), String> {
        Ok(())
    }
}

pub(crate) struct Checker {
    world: World,
    mode: Box<dyn ModeChecks>,
    /// The commands of the first reply to every hold.
    holds: BTreeMap<(i32, u32), Vec<DriverCommand>>,
    /// The standings after the input before.
    standings: DriverStandings,
    /// The status of every player of the match.
    statuses: BTreeMap<String, DriverStatus>,
    match_ended: bool,
    playlist: u32,
}

impl Checker {
    pub(crate) fn new(world: &World) -> Result<Self, String> {
        let settings = if world.settings.trim().is_empty() {
            "{}"
        } else {
            &world.settings
        };
        let parse = |error: serde_json::Error| format!("Invalid settings: {error}");
        let mode: Box<dyn ModeChecks> = match world.mode {
            Mode::Rounds => Box::new(rounds::Rounds::new(
                serde_json::from_str(settings).map_err(parse)?,
            )),
            Mode::TimeAttack => Box::new(time_attack::TimeAttack::new(
                serde_json::from_str(settings).map_err(parse)?,
            )),
            Mode::ReverseCup => Box::new(reverse_cup::ReverseCup::new(
                serde_json::from_str(settings).map_err(parse)?,
            )),
            Mode::Knockout => Box::new(knockout::Knockout::new(
                serde_json::from_str(settings).map_err(parse)?,
            )),
        };
        Ok(Self {
            world: world.clone(),
            mode,
            holds: BTreeMap::new(),
            standings: DriverStandings::default(),
            statuses: BTreeMap::new(),
            match_ended: false,
            playlist: 0,
        })
    }

    /// Checks the reply to an input and the driver after it.
    pub(crate) fn after(
        &mut self,
        input: &DriverInput,
        reply: &DriverReply,
        driver: &Driver,
        server: &Server,
    ) -> Result<(), String> {
        let standings = driver.standings();
        let result = self.check(input, reply, driver, server, &standings);
        self.standings = standings;
        result
    }

    fn check(
        &mut self,
        input: &DriverInput,
        reply: &DriverReply,
        driver: &Driver,
        server: &Server,
        standings: &DriverStandings,
    ) -> Result<(), String> {
        match input {
            DriverInput::Event(Event::StartMatchStart(_)) => {
                self.statuses.clear();
                self.match_ended = false;
                self.mode.match_start();
            }
            DriverInput::Playlist(uids) => self.playlist = uids.len() as u32,
            _ => {}
        }

        let hold = match input {
            DriverInput::Event(Event::DriverHold(hold)) => Some(hold),
            _ => None,
        };
        let first = match hold {
            Some(hold) => {
                let continues = reply
                    .commands
                    .iter()
                    .filter(|command| matches!(command, DriverCommand::Continue(_)))
                    .count();
                if continues != 1
                    || reply.commands.last() != Some(&DriverCommand::Continue(hold.seq))
                {
                    return Err(format!(
                        "The reply to hold {} does not end with its only Continue",
                        hold.seq
                    ));
                }
                match self.holds.get(&(hold.session, hold.seq)) {
                    Some(commands) => {
                        if *commands != reply.commands {
                            return Err(format!(
                                "Hold {} got other commands when it came again",
                                hold.seq
                            ));
                        }
                        false
                    }
                    None => {
                        self.holds
                            .insert((hold.session, hold.seq), reply.commands.clone());
                        true
                    }
                }
            }
            None => {
                if let Some(command) = reply.commands.iter().find(|command| {
                    matches!(
                        command,
                        DriverCommand::Continue(_)
                            | DriverCommand::EndMap
                            | DriverCommand::EndMatch
                    )
                }) {
                    return Err(format!("{command:?} outside of a hold"));
                }
                false
            }
        };

        let counts = first
            && hold.is_some_and(|hold| hold.section == DriverSection::RoundEnd && !hold.forced);
        if reply.round_result.is_some() != counts {
            return Err(if counts {
                "A round that counted has no result for the manager".into()
            } else {
                "A result for the manager without a round that counted".into()
            });
        }

        self.check_commands(&reply.commands, driver)?;
        check_standings(standings)?;
        let round = driver.last_round();
        check_round(&round, standings)?;
        self.check_statuses(standings, server)?;
        check_users(&driver.users(), server)?;

        if first && let Some(hold) = hold {
            match hold.section {
                DriverSection::RoundEnd if !hold.forced => {
                    let end = RoundEnd {
                        before: &self.standings,
                        after: standings,
                        round: &round,
                        commands: &reply.commands,
                        truth: server.truth(),
                        ended: self.match_ended,
                    };
                    self.mode.round_end(&end)?;
                }
                DriverSection::MapEnd => {
                    let end = MapEnd {
                        after: standings,
                        commands: &reply.commands,
                        playlist: self.playlist,
                    };
                    self.mode.map_end(&end)?;
                }
                _ => {}
            }
        }
        if ends_match(&reply.commands) {
            self.match_ended = true;
        }
        Ok(())
    }

    /// The commands are valid for the server, and what the driver publishes is what
    /// it has.
    fn check_commands(&self, commands: &[DriverCommand], driver: &Driver) -> Result<(), String> {
        for command in commands {
            match command {
                DriverCommand::SetNextMap(uid) if self.world.map(uid).is_none() => {
                    return Err(format!("The next map {uid} is not on the server"));
                }
                DriverCommand::SetStandings(json) => {
                    let published: DriverStandings = parse(json)?;
                    if published != driver.standings() {
                        return Err("The published standings differ from the driver's".into());
                    }
                }
                DriverCommand::SetRound(json) => {
                    let published: DriverRound = parse(json)?;
                    if published != driver.last_round() {
                        return Err("The published round differs from the driver's".into());
                    }
                }
                DriverCommand::SetUsers(json) => {
                    let published: DriverUsers = parse(json)?;
                    if published != driver.users() {
                        return Err("The published players differ from the driver's".into());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Nobody leaves the standings of a match, eliminated players and spectators stay
    /// what they are, and none of them can spawn.
    fn check_statuses(
        &mut self,
        standings: &DriverStandings,
        server: &Server,
    ) -> Result<(), String> {
        for (account_id, before) in &self.statuses {
            let Some(entry) = standings
                .entries
                .iter()
                .find(|e| &e.account_id == account_id)
            else {
                return Err(format!("{account_id} left the standings of the match"));
            };
            let allowed = match before {
                DriverStatus::Playing => true,
                DriverStatus::Eliminated => entry.status == DriverStatus::Eliminated,
                DriverStatus::Spectator => entry.status == DriverStatus::Spectator,
            };
            if !allowed {
                return Err(format!(
                    "{account_id} went from {before:?} to {:?}",
                    entry.status
                ));
            }
        }
        for entry in &standings.entries {
            self.statuses.insert(entry.account_id.clone(), entry.status);
            if entry.status != DriverStatus::Playing && server.can_spawn(&entry.account_id) {
                return Err(format!(
                    "{} is {:?} but can still spawn",
                    entry.account_id, entry.status
                ));
            }
        }
        Ok(())
    }
}

/// Every player has one entry, the positions go from 1 without gaps and players
/// still playing rank before eliminated ones.
fn check_standings(standings: &DriverStandings) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for entry in &standings.entries {
        if !seen.insert(&entry.account_id) {
            return Err(format!("{} is in the standings twice", entry.account_id));
        }
        if entry.status == DriverStatus::Spectator && entry.position != 0 {
            return Err(format!("Spectator {} has a position", entry.account_id));
        }
    }

    let positions: Vec<u32> = standings
        .entries
        .iter()
        .map(|entry| entry.position)
        .filter(|&position| position > 0)
        .collect();
    if !positions.iter().copied().eq(1..=positions.len() as u32) {
        return Err(format!(
            "The positions are not 1 to n in order: {positions:?}"
        ));
    }
    // The positions come first in the published order.
    if standings
        .entries
        .windows(2)
        .any(|pair| pair[0].position == 0 && pair[1].position > 0)
    {
        return Err("A player without a position comes before a ranked one".into());
    }

    let worst_playing = standings
        .entries
        .iter()
        .filter(|entry| entry.status == DriverStatus::Playing)
        .map(|entry| entry.position)
        .max();
    let best_eliminated = standings
        .entries
        .iter()
        .filter(|entry| entry.status == DriverStatus::Eliminated)
        .map(|entry| entry.position)
        .min();
    if let (Some(playing), Some(eliminated)) = (worst_playing, best_eliminated)
        && playing >= eliminated
    {
        return Err(format!(
            "Position {playing} plays on behind eliminated position {eliminated}"
        ));
    }
    if best_eliminated == Some(0) {
        return Err("An eliminated player has no position".into());
    }
    Ok(())
}

/// The places of the last round go from 1 in order, for players taking part.
fn check_round(round: &DriverRound, standings: &DriverStandings) -> Result<(), String> {
    if !round
        .entries
        .iter()
        .map(|entry| entry.position)
        .eq(1..=round.entries.len() as u32)
    {
        return Err("The places of the last round are not 1 to n in order".into());
    }
    for entry in &round.entries {
        let status = standings
            .entries
            .iter()
            .find(|e| e.account_id == entry.account_id)
            .map(|e| e.status);
        if matches!(status, None | Some(DriverStatus::Spectator)) {
            return Err(format!(
                "{} got a place in the round without taking part",
                entry.account_id
            ));
        }
    }
    Ok(())
}

/// The driver knows the name of everyone the server told it about.
fn check_users(users: &DriverUsers, server: &Server) -> Result<(), String> {
    for (account_id, name) in server.known() {
        if !users
            .users
            .iter()
            .any(|user| &user.account_id == account_id && &user.name == name)
        {
            return Err(format!("The driver does not know {account_id} as {name}"));
        }
    }
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("Published json is invalid: {e}"))
}

pub(crate) fn ends_match(commands: &[DriverCommand]) -> bool {
    commands.contains(&DriverCommand::EndMatch)
}

pub(crate) fn ends_map(commands: &[DriverCommand]) -> bool {
    commands.contains(&DriverCommand::EndMap)
}

/// The entries still in the match.
pub(crate) fn playing(
    standings: &DriverStandings,
) -> impl Iterator<Item = &tm_server_types::method::DriverEntry> {
    standings
        .entries
        .iter()
        .filter(|entry| entry.status == DriverStatus::Playing)
}

pub(crate) fn entry<'a>(
    standings: &'a DriverStandings,
    account_id: &str,
) -> Option<&'a tm_server_types::method::DriverEntry> {
    standings
        .entries
        .iter()
        .find(|entry| entry.account_id == account_id)
}

/// Points of every finisher by place: the value of the place, the last one beyond.
pub(crate) fn points(repartition: &[i32], place: usize) -> i32 {
    repartition
        .get(place)
        .or(repartition.last())
        .copied()
        .unwrap_or(0)
}

/// The round ranks the players in the match by points, the highest first.
pub(crate) fn check_ranked_by_points(after: &DriverStandings) -> Result<(), String> {
    let mut ranked: Vec<_> = playing(after).filter(|entry| entry.position > 0).collect();
    ranked.sort_by_key(|entry| entry.position);
    if let Some(pair) = ranked.windows(2).find(|pair| pair[0].score < pair[1].score) {
        return Err(format!(
            "{} ranks before {} with fewer points",
            pair[0].account_id, pair[1].account_id
        ));
    }
    Ok(())
}
