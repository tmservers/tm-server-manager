//! What has to hold after every input, whatever the mode, and when the rules of
//! the mode are checked.

use std::collections::{BTreeMap, BTreeSet};

use tm_server_types::{
    event::{DriverSection, Event},
    method::{DriverCommand, DriverEntry, DriverRound, DriverStandings, DriverStatus, DriverUsers},
};
use tm_wasm_driver::{Driver, DriverInput, DriverReply};

use crate::{
    World,
    rules::{MapEnd, RoundEnd, Rules, ends_match},
    server::Server,
};

pub(crate) struct Checker {
    world: World,
    rules: Box<dyn Rules>,
    /// The commands of the first reply to every hold.
    holds: BTreeMap<(i32, u32), Vec<DriverCommand>>,
    /// The standings after the last input that changed something.
    standings: DriverStandings,
    /// The status of every player of the match.
    statuses: BTreeMap<String, DriverStatus>,
    match_ended: bool,
    playlist: u32,
}

impl Checker {
    pub(crate) fn new(world: &World, rules: Box<dyn Rules>) -> Self {
        Self {
            world: world.clone(),
            rules,
            holds: BTreeMap::new(),
            standings: DriverStandings::default(),
            statuses: BTreeMap::new(),
            match_ended: false,
            playlist: 0,
        }
    }

    /// Checks the reply to an input and the driver after it.
    pub(crate) fn after(
        &mut self,
        input: &DriverInput,
        reply: &DriverReply,
        driver: &Driver,
        server: &Server,
    ) -> Result<(), String> {
        match input {
            DriverInput::Event(Event::StartMatchStart(_)) => {
                self.statuses.clear();
                self.match_ended = false;
                self.rules.match_start();
            }
            DriverInput::Playlist(uids) => self.playlist = uids.len() as u32,
            _ => {}
        }

        let hold = match input {
            DriverInput::Event(Event::DriverHold(hold)) => Some(hold),
            _ => None,
        };
        let first = match hold {
            Some(hold) => self.check_hold(hold.session, hold.seq, reply)?,
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

        // The driver publishes whatever changes, so the rest only has to be checked
        // after replies with commands. Most waypoints have none.
        let learned = matches!(
            input,
            DriverInput::Players(_) | DriverInput::Event(Event::PlayerInfoChanged(_))
        );
        if reply.commands.is_empty() && !learned {
            return Ok(());
        }

        let standings = driver.standings();
        let round = driver.last_round();
        self.check_commands(&reply.commands, driver, &standings, &round)?;
        check_standings(&standings)?;
        check_round(&round, &standings)?;
        self.check_statuses(&standings, server)?;
        check_users(&driver.users(), server)?;

        if first && let Some(hold) = hold {
            match hold.section {
                DriverSection::RoundEnd if !hold.forced => {
                    let end = RoundEnd {
                        before: &self.standings,
                        after: &standings,
                        round: &round,
                        commands: &reply.commands,
                        truth: server.truth(),
                        ended: self.match_ended,
                    };
                    self.rules.round_end(&end)?;
                }
                DriverSection::MapEnd => {
                    let end = MapEnd {
                        after: &standings,
                        commands: &reply.commands,
                        playlist: self.playlist,
                    };
                    self.rules.map_end(&end)?;
                }
                _ => {}
            }
        }
        if ends_match(&reply.commands) {
            self.match_ended = true;
        }
        self.standings = standings;
        Ok(())
    }

    /// A reply to a hold ends with its only `Continue`, and a hold that comes again
    /// gets the same commands. True for the first reply.
    fn check_hold(&mut self, session: i32, seq: u32, reply: &DriverReply) -> Result<bool, String> {
        let continues = reply
            .commands
            .iter()
            .filter(|command| matches!(command, DriverCommand::Continue(_)))
            .count();
        if continues != 1 || reply.commands.last() != Some(&DriverCommand::Continue(seq)) {
            return Err(format!(
                "The reply to hold {seq} does not end with its only Continue"
            ));
        }
        match self.holds.get(&(session, seq)) {
            Some(commands) if *commands != reply.commands => {
                Err(format!("Hold {seq} got other commands when it came again"))
            }
            Some(_) => Ok(false),
            None => {
                self.holds.insert((session, seq), reply.commands.clone());
                Ok(true)
            }
        }
    }

    /// The commands are valid for the server, and what the driver publishes is what
    /// it has.
    fn check_commands(
        &self,
        commands: &[DriverCommand],
        driver: &Driver,
        standings: &DriverStandings,
        round: &DriverRound,
    ) -> Result<(), String> {
        for command in commands {
            let (published, differs) = match command {
                DriverCommand::SetNextMap(uid) if self.world.map(uid).is_none() => {
                    return Err(format!("The next map {uid} is not on the server"));
                }
                DriverCommand::SetStandings(json) => {
                    ("standings", parse::<DriverStandings>(json)? != *standings)
                }
                DriverCommand::SetRound(json) => ("round", parse::<DriverRound>(json)? != *round),
                DriverCommand::SetUsers(json) => {
                    ("players", parse::<DriverUsers>(json)? != driver.users())
                }
                _ => continue,
            };
            if differs {
                return Err(format!(
                    "The published {published} differ from the driver's"
                ));
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
        let entries: BTreeMap<&str, &DriverEntry> = standings
            .entries
            .iter()
            .map(|entry| (entry.account_id.as_str(), entry))
            .collect();
        for (account_id, before) in &self.statuses {
            let Some(entry) = entries.get(account_id.as_str()) else {
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
    let statuses: BTreeMap<&str, DriverStatus> = standings
        .entries
        .iter()
        .map(|entry| (entry.account_id.as_str(), entry.status))
        .collect();
    for entry in &round.entries {
        let status = statuses.get(entry.account_id.as_str());
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
    let names: BTreeMap<&str, &str> = users
        .users
        .iter()
        .map(|user| (user.account_id.as_str(), user.name.as_str()))
        .collect();
    for (account_id, name) in server.known() {
        if names.get(account_id.as_str()) != Some(&name.as_str()) {
            return Err(format!("The driver does not know {account_id} as {name}"));
        }
    }
    Ok(())
}

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, String> {
    serde_json::from_str(json).map_err(|e| format!("Published json is invalid: {e}"))
}
