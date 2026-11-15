//! What the driver of a match got, as the bridge streams it to the manager.
//!
//! The bridge runs the driver and decides, the manager replays the same records to
//! compute the scores of the match. Both turn the records back into the inputs of the
//! driver the same way, so both drivers are the same. Starts and checkpoints are most
//! of a match and only carry what is new: a checkpoint is 11 bytes.

use crate::{base::PlayerInfo, event::Event};

/// One input of the driver of a match, numbered in the order the driver got them.
///
/// Variants are only ever appended, the manager stores them.
// Nearly every record is small, boxing the events would only add allocations.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverRecord {
    /// A callback of the server the driver handles, which is no start or waypoint.
    Event(Event),
    /// The account of the player the records refer to by a number.
    Player(DriverRecordPlayer),
    /// A player started a race.
    Start(DriverRecordStart),
    /// A player passed a checkpoint, which ends no lap.
    Checkpoint(DriverRecordCheckpoint),
    /// A player passed the line which ends a lap, but not the race.
    LapEnd(DriverRecordLapEnd),
    /// A player finished the race.
    Finish(DriverRecordLapEnd),
    /// What the race of a player is, before a checkpoint whose race the records do
    /// not tell, e.g. because the server did not send its start or a checkpoint.
    Resync(DriverRecordResync),
    /// The race of a player started at this time, not the one the records tell.
    /// Comes before a checkpoint, the replay takes the time of the checkpoint from it.
    Clock(DriverRecordStart),
    /// A player respawned, only for the statistics of the race.
    Respawn(u32),
    /// A player gave up, only for the statistics of the race.
    GiveUp(u32),
    /// The uids of the maps the match plays from now on.
    Playlist(Vec<String>),
    /// The players on the server, e.g. when the driver starts or resumes.
    Players(Vec<PlayerInfo>),
    /// The reply to the last hold reached the server.
    Delivered,
    /// The driver resumed and replies to the last records may have been lost.
    Resumed,
    /// New json settings for the match.
    Settings(String),
    /// What the standings of the bridge are after a round counted, to find out
    /// whether the manager computes the same.
    Digest(DriverRecordDigest),
    /// The teams of the match and who plays for them.
    Teams(Vec<DriverTeam>),
}

/// A team of a match, as the manager knows it from the entrants of the match.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverTeam {
    pub team_id: u32,
    pub name: String,
    /// E.g. a tag of a few letters. Empty while the manager has none.
    pub short_name: String,
    pub logo_url: String,
    pub flag_url: String,
    /// The accounts of the players.
    pub players: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRecordPlayer {
    /// From 0, in the order the bridge first saw the players in the match.
    pub player: u32,
    pub account_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRecordStart {
    pub player: u32,
    /// Server time of the start.
    pub time: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRecordCheckpoint {
    pub player: u32,
    /// Race time at the checkpoint.
    pub race_time: u32,
    /// Of the checkpoint in the race, from 0.
    pub index: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRecordLapEnd {
    pub player: u32,
    pub race_time: u32,
    pub index: u16,
    /// Checkpoints of a lap, the line that ends it included. 0 for a finish which
    /// ends no lap.
    pub lap_length: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRecordResync {
    pub player: u32,
    /// Server time the race started.
    pub start: u32,
    /// Race times of the checkpoints of the race so far.
    pub checkpoints: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRecordDigest {
    /// Rounds of the match that counted.
    pub rounds: u32,
    /// Hash of the standings.
    pub standings: u64,
}
