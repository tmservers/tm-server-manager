//! The rules of a mode, which a simulation checks at the end of every round and
//! map. A mode implements [`Rules`] in its tests and hands it to the
//! [`Simulator`](crate::Simulator).
//!
//! Rules test properties against the races the simulation knows, e.g. that the
//! players who lost a life in a knockout round are the last ones of its race. They
//! do not compute the standings a second way.

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

pub fn entry<'a>(standings: &'a DriverStandings, account_id: &str) -> Option<&'a DriverEntry> {
    standings
        .entries
        .iter()
        .find(|entry| entry.account_id == account_id)
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
