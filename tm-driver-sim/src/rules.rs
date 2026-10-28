//! The rules of a mode, which a simulation checks at the end of every round and
//! map. A mode implements [`Rules`] in its tests and hands it to the
//! [`Simulator`](crate::Simulator).
//!
//! Rules test properties against the races the simulation knows, e.g. that the
//! players who lost a life in a knockout round are the last ones of its race. They
//! do not compute the standings a second way. The order of a race is the one
//! exception every mode shares, see [`in_race_order`].

use std::cmp::Reverse;

use serde::de::DeserializeOwned;
pub use tm_server_types::method::{
    DriverCommand, DriverEntry, DriverRound, DriverStandings, DriverStatus,
};

pub use crate::observe::{RoundTruth, SeenRace};

pub trait Rules {
    /// A match starts, e.g. to forget the points of the last one.
    fn match_start(&mut self) {}

    /// A round counted.
    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String>;

    fn map_end(&mut self, _end: &MapEnd) -> Result<(), String> {
        Ok(())
    }

    /// The most rounds that place players a match with these standings can take
    /// before it ends, e.g. the lives of every participant in a knockout. A match
    /// that goes on longer fails, so a mode that stops making progress shows up.
    /// None for a match that may go on, e.g. one waiting for players.
    fn round_limit(&self, _standings: &DriverStandings) -> Option<u32> {
        None
    }
}

/// Nothing beyond what holds for every mode.
pub(crate) struct NoRules;

impl Rules for NoRules {
    fn round_end(&mut self, _end: &RoundEnd) -> Result<(), String> {
        Ok(())
    }
}

/// A round that counted.
pub struct RoundEnd<'a> {
    /// The standings before the round ended.
    pub before: &'a DriverStandings,
    pub after: &'a DriverStandings,
    pub round: &'a DriverRound,
    /// What the driver answered the round end with.
    pub commands: &'a [DriverCommand],
    /// The races of the round as the driver saw them.
    pub truth: &'a RoundTruth,
}

/// A map that ends.
pub struct MapEnd<'a> {
    pub after: &'a DriverStandings,
    pub commands: &'a [DriverCommand],
    /// Maps in the playlist of the driver.
    pub playlist: u32,
}

/// Which race of a player counts in a round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counted {
    /// The last finished race, in rounds of one try.
    Last,
    /// The fastest finished race, the earlier one of equal times, in time attack.
    Best,
}

/// The settings of a mode the way it parses them, `{}` when there are none.
pub fn settings<T: DeserializeOwned>(json: &str) -> Result<T, String> {
    let json = if json.trim().is_empty() { "{}" } else { json };
    serde_json::from_str(json).map_err(|e| format!("Invalid settings: {e}"))
}

pub fn ends_match(commands: &[DriverCommand]) -> bool {
    commands.contains(&DriverCommand::EndMatch)
}

pub fn ends_map(commands: &[DriverCommand]) -> bool {
    commands.contains(&DriverCommand::EndMap)
}

/// The players still in the match.
pub fn playing(standings: &DriverStandings) -> impl Iterator<Item = &DriverEntry> {
    standings
        .entries
        .iter()
        .filter(|entry| entry.status == DriverStatus::Playing)
}

/// Everyone taking part in the match, eliminated or not.
pub fn participants(standings: &DriverStandings) -> impl Iterator<Item = &DriverEntry> {
    standings
        .entries
        .iter()
        .filter(|entry| entry.status != DriverStatus::Spectator)
}

pub fn entry<'a>(standings: &'a DriverStandings, account_id: &str) -> Option<&'a DriverEntry> {
    standings
        .entries
        .iter()
        .find(|entry| entry.account_id == account_id)
}

/// Points of a place by a repartition: the value of the place, the last one beyond.
pub fn points(repartition: &[i32], place: usize) -> i32 {
    repartition
        .get(place)
        .or(repartition.last())
        .copied()
        .unwrap_or(0)
}

/// The players in the match rank by their points, the most first.
pub fn ranked_by_points(standings: &DriverStandings) -> Result<(), String> {
    let mut ranked: Vec<&DriverEntry> = playing(standings)
        .filter(|entry| entry.position > 0)
        .collect();
    ranked.sort_by_key(|entry| entry.position);
    if let Some(pair) = ranked.windows(2).find(|pair| pair[0].score < pair[1].score) {
        return Err(format!(
            "{} ranks before {} with fewer points",
            pair[0].account_id, pair[1].account_id
        ));
    }
    Ok(())
}

/// The round places its players in the order of their races, which every mode
/// shares: finishers by the time of the race that counts, then everyone else by
/// the checkpoints of their last race, players who did not race last. Equal races
/// go to the better time at the last checkpoint, then the one before and so on,
/// and finally to the account. Nobody ties.
pub fn in_race_order(end: &RoundEnd, counted: Counted) -> Result<(), String> {
    for pair in end.round.entries.windows(2) {
        let (a, b) = (&pair[0].account_id, &pair[1].account_id);
        if race_key(end.truth, a, counted) > race_key(end.truth, b, counted) {
            return Err(format!("{a} is placed before {b}, who raced better"));
        }
    }
    Ok(())
}

/// What orders a player in a race, the best first.
fn race_key<'a>(
    truth: &RoundTruth,
    account_id: &'a str,
    counted: Counted,
) -> (bool, Option<u32>, Reverse<Option<usize>>, Vec<u32>, &'a str) {
    let races = truth.races(account_id).unwrap_or_default();
    let mut finished = races.iter().filter(|race| race.time().is_some());
    let counted_race = match counted {
        Counted::Last => finished.next_back(),
        Counted::Best => finished.min_by_key(|race| race.time()),
    };
    // The race that counts, the last one without a finish.
    let race = counted_race.or(races.last());
    let time = race.and_then(SeenRace::time);
    let checkpoints_backwards = race.map_or_else(Vec::new, |race| {
        race.checkpoints.iter().rev().copied().collect()
    });
    (
        time.is_none(),
        time,
        Reverse(race.map(|race| race.checkpoints.len())),
        checkpoints_backwards,
        account_id,
    )
}

/// Everyone who raced and takes part in the match has a place in the round.
pub fn racers_placed(end: &RoundEnd) -> Result<(), String> {
    for (account_id, _) in &end.truth.racers {
        let takes_part = entry(end.after, account_id)
            .is_some_and(|entry| entry.status != DriverStatus::Spectator);
        if takes_part
            && !end
                .round
                .entries
                .iter()
                .any(|entry| &entry.account_id == account_id)
        {
            return Err(format!("{account_id} raced but has no place in the round"));
        }
    }
    Ok(())
}

/// Spectators of a match stay spectators, e.g. players who came too late to take
/// part.
pub fn spectators_stay(end: &RoundEnd) -> Result<(), String> {
    for before in end
        .before
        .entries
        .iter()
        .filter(|entry| entry.status == DriverStatus::Spectator)
    {
        if let Some(after) = entry(end.after, &before.account_id)
            && after.status != DriverStatus::Spectator
        {
            return Err(format!(
                "Spectator {} went to {:?}",
                before.account_id, after.status
            ));
        }
    }
    Ok(())
}
