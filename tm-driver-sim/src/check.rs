//! What has to hold after every input, whatever the mode, and when the rules of
//! the mode are checked.

use std::collections::{BTreeMap, BTreeSet};

use tm_server_types::{
    event::{DriverSection, Event},
    method::{DriverCommand, DriverEntry, DriverRound, DriverStandings, DriverStatus, DriverUsers},
};
use tm_wasm_driver::{Driver, DriverInput, DriverReply};

use crate::{
    observe::{Observer, SeenHold},
    rules::{MapEnd, RoundEnd, Rules},
};

pub(crate) struct Checker {
    rules: Box<dyn Rules>,
    /// The maps of the server, None when unknown, e.g. for a log.
    maps: Option<BTreeSet<String>>,
    /// The standings after the last input that changed something.
    standings: DriverStandings,
    /// The status of every player of the match.
    statuses: BTreeMap<String, DriverStatus>,
    playlist: u32,
}

impl Checker {
    pub(crate) fn new(rules: Box<dyn Rules>, maps: Option<BTreeSet<String>>) -> Self {
        Self {
            rules,
            maps,
            standings: DriverStandings::default(),
            statuses: BTreeMap::new(),
            playlist: 0,
        }
    }

    /// Checks the reply to an input and the driver after it. `reached` is false when
    /// the reply never reached the server, which then still shows what it had.
    pub(crate) fn after(
        &mut self,
        input: &DriverInput,
        hold: Option<&SeenHold>,
        reply: &DriverReply,
        reached: bool,
        driver: &Driver,
        observer: &Observer,
    ) -> Result<(), String> {
        match input {
            DriverInput::Event(Event::StartMatchStart(_)) => {
                self.statuses.clear();
                self.rules.match_start();
            }
            DriverInput::Playlist(uids) => self.playlist = uids.len() as u32,
            _ => {}
        }

        match hold {
            Some(hold) => check_hold(hold, reply)?,
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
            }
        }

        let counts = hold.is_some_and(|hold| hold.counts);
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
        let users = driver.users();
        self.check_next_map(&reply.commands)?;
        check_standings(&standings)?;
        check_round(&round, &standings)?;
        check_statuses(&mut self.statuses, &standings)?;
        check_users(&users, observer)?;
        if reached {
            check_published(observer, &standings, &round, &users)?;
            check_spawns(&standings, observer)?;
        }

        if let Some(hold) = hold.filter(|hold| hold.first) {
            match hold.hold.section {
                DriverSection::RoundEnd if hold.counts => {
                    let end = RoundEnd {
                        before: &self.standings,
                        after: &standings,
                        round: &round,
                        commands: &reply.commands,
                        truth: observer.truth(),
                    };
                    self.rules.round_end(&end)?;
                }
                DriverSection::RoundEnd => check_unchanged(&self.standings, &standings)?,
                DriverSection::MapEnd => {
                    let end = MapEnd {
                        after: &standings,
                        commands: &reply.commands,
                        playlist: self.playlist,
                    };
                    self.rules.map_end(&end)?;
                }
                DriverSection::MapStart => {}
            }
        }
        self.standings = standings;
        Ok(())
    }

    fn check_next_map(&self, commands: &[DriverCommand]) -> Result<(), String> {
        for command in commands {
            if let (DriverCommand::SetNextMap(uid), Some(maps)) = (command, &self.maps)
                && !maps.contains(uid)
            {
                return Err(format!("The next map {uid} is not on the server"));
            }
        }
        Ok(())
    }
}

/// A reply to a hold ends with its only `Continue`, and a hold that comes again
/// gets the same commands.
fn check_hold(hold: &SeenHold, reply: &DriverReply) -> Result<(), String> {
    let seq = hold.hold.seq;
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
    match &hold.answered {
        Some(commands) if *commands != reply.commands => {
            Err(format!("Hold {seq} got other commands when it came again"))
        }
        _ => Ok(()),
    }
}

/// The server shows what the driver has: the driver published every change.
fn check_published(
    observer: &Observer,
    standings: &DriverStandings,
    round: &DriverRound,
    users: &DriverUsers,
) -> Result<(), String> {
    let published = observer.published();
    let stale = if published.standings.as_ref() != Some(standings) {
        "standings"
    } else if published.round.as_ref() != Some(round) {
        "last round"
    } else if published.users.as_ref() != Some(users) {
        "players"
    } else {
        return Ok(());
    };
    Err(format!(
        "The server shows other {stale} than the driver has"
    ))
}

/// Nobody leaves the standings of a match and eliminated players stay eliminated.
fn check_statuses(
    statuses: &mut BTreeMap<String, DriverStatus>,
    standings: &DriverStandings,
) -> Result<(), String> {
    let entries: BTreeMap<&str, &DriverEntry> = standings
        .entries
        .iter()
        .map(|entry| (entry.account_id.as_str(), entry))
        .collect();
    for (account_id, before) in statuses.iter() {
        let Some(entry) = entries.get(account_id.as_str()) else {
            return Err(format!("{account_id} left the standings of the match"));
        };
        if *before == DriverStatus::Eliminated && entry.status != DriverStatus::Eliminated {
            return Err(format!(
                "{account_id} went from Eliminated to {:?}",
                entry.status
            ));
        }
    }
    for entry in &standings.entries {
        statuses.insert(entry.account_id.clone(), entry.status);
    }
    Ok(())
}

/// Players still playing can spawn, everyone else cannot.
fn check_spawns(standings: &DriverStandings, observer: &Observer) -> Result<(), String> {
    for entry in &standings.entries {
        let playing = entry.status == DriverStatus::Playing;
        if playing != observer.can_spawn(&entry.account_id) {
            let can = if playing { "cannot" } else { "can still" };
            return Err(format!(
                "{} is {:?} but {can} spawn",
                entry.account_id, entry.status
            ));
        }
    }
    Ok(())
}

/// A round that did not count changes nobody's points or status. Players new to the
/// match may show up.
fn check_unchanged(before: &DriverStandings, after: &DriverStandings) -> Result<(), String> {
    for old in &before.entries {
        let Some(new) = after
            .entries
            .iter()
            .find(|entry| entry.account_id == old.account_id)
        else {
            continue;
        };
        if (new.score, new.status) != (old.score, old.status) {
            return Err(format!(
                "A round that did not count changed {} from {} {:?} to {} {:?}",
                old.account_id, old.score, old.status, new.score, new.status
            ));
        }
    }
    Ok(())
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
fn check_users(users: &DriverUsers, observer: &Observer) -> Result<(), String> {
    let names: BTreeMap<&str, &str> = users
        .users
        .iter()
        .map(|user| (user.account_id.as_str(), user.name.as_str()))
        .collect();
    for (account_id, name) in observer.known() {
        if names.get(account_id.as_str()) != Some(&name.as_str()) {
            return Err(format!("The driver does not know {account_id} as {name}"));
        }
    }
    Ok(())
}
