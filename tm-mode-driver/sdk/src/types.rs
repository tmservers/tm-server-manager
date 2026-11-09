//! What the driver host and a mode exchange, the same for every engine that runs modes.
//!
//! Events are built from the standard callbacks of the server. The host handles
//! the transport details of the driver modescript: waiting holds and letting them
//! continue.

use serde::{Deserialize, Serialize};

/// A race of a player, from a start line to the finish, a give up, a restart or
/// the end of the round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Race {
    /// The race times at the checkpoints passed in milliseconds, the finish
    /// included. How many there are tells how far the player got, the times
    /// tell players who got equally far apart.
    pub checkpoints: Vec<u32>,
    /// The race reached the finish. Its race time is the last checkpoint time.
    pub finished: bool,
    /// Laps the race completed. On maps without laps the finish completes the
    /// only lap.
    pub laps: u32,
}

/// A player who started a race in the round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Racer {
    pub account_id: String,
    /// Every race of the player in the round in the order they started them,
    /// e.g. one per restart in time attack rounds.
    pub races: Vec<Race>,
}

/// The races of a round as they happened. Which race of a player counts is up
/// to the mode. While the round runs, the live round gets the races so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundEnd {
    /// The round does not count: it was forced to end or skipped (e.g. after a
    /// pause), or the host did not see it start, e.g. after the bridge restarted.
    pub invalid: bool,
    /// Everyone who started a race in the round in the order of their first
    /// start, also players who left since.
    pub racers: Vec<Racer>,
    /// The players on the server when the round ended, racers or not.
    pub present: Vec<String>,
}

/// Everything a mode gets told about.
///
/// The server waits for the commands of [`Event::MapStart`], [`Event::RoundEnd`]
/// and [`Event::MapEnd`] before it continues. The other events are informative.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    MatchStart,
    MapStart,
    RoundStart,
    /// The account id of a player who started their first race of the current
    /// round, e.g. so the standings show everyone who plays before the first
    /// round ends.
    RaceStart(String),
    RoundEnd(RoundEnd),
    MapEnd,
    MatchEnd,
    /// The mode took new settings during the match, e.g. to show them in the info
    /// and send the round format again. No round runs.
    SettingsChanged,
}

/// What the host knows about a player. Read only metadata, a mode asks for it
/// with [`crate::Host::user`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub account_id: String,
    /// Empty until the host got the name from the server.
    pub name: String,
    /// Empty while unknown.
    pub club_tag: String,
    /// E.g. `World|Europe|Germany`. Empty while unknown.
    pub zone: String,
}

/// Whether a player takes part in the match and can still race. Who wins and
/// loses is told by the position, what a mode calls a state (e.g. last chance)
/// by the label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Takes part and can still race.
    Playing,
    /// Out of the match. The position the mode gave the player at the elimination is final.
    Eliminated,
    /// Takes no part in the match, e.g. joined too late. Not ranked and left out
    /// of round results.
    Spectator,
}

/// How a label is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tone {
    Neutral,
    /// Something good, e.g. a win.
    Positive,
    /// Something bad, e.g. being one round from elimination.
    Negative,
}

/// Shown in place of the points, e.g. `Last Chance` or `Winner`. Only for display,
/// the host and the manialinks know nothing about its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub text: String,
    pub tone: Tone,
}

/// A player in the standings of the match or of a round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub account_id: String,
    /// Rank in the match or place in the round, from 1. Nobody shares a position.
    /// 0 until the mode sets it, e.g. for players who just joined.
    pub position: u32,
    /// Points of the match or of the round.
    pub score: i32,
    /// E.g. the race that counted in the round, in milliseconds.
    pub time: Option<u32>,
    pub status: Status,
    pub label: Option<Label>,
}

/// The scores of a match in one structure every mode shares.
///
/// The host keeps the standings and hands them to the mode with every event.
/// What the mode returns is shown to the players and reported to the manager,
/// so a mode keeps no scores of its own. The host clears what belongs to a new
/// match or map before the mode gets the event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standings {
    /// The match: one entry for every player taking part. Kept between events.
    pub entries: Vec<Entry>,
    /// The round that ended with this event, filled by the mode at the round end.
    /// The host takes it out after every event: it shows it as the last round
    /// and reports it to the manager. Ignored for invalid rounds. The round that
    /// runs comes from the live round.
    pub round: Vec<Entry>,
    /// Rounds of the current map that counted, as the mode counts them. The host
    /// clears them when a map starts.
    pub rounds_on_map: u32,
    /// Valid rounds of the match. The host counts the ending round before the
    /// round end.
    pub rounds_played: u32,
    /// Maps that ended in this match. The host counts the ending map before the
    /// map end.
    pub maps_played: u32,
    /// Progress of the match shown with the standings, e.g. `2 / 5 rounds`.
    pub info: String,
}

/// How long the remaining players have to finish once the first player finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishTimeout {
    /// 5 seconds plus a sixth of the author time.
    Automatic,
    /// The round only ends once every player finished or gave up.
    Disabled,
    Milliseconds(u32),
}

/// How rounds are played.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoundFormat {
    /// One attempt per player. The round ends once nobody is racing anymore or the
    /// finish timeout ran out. Automatic is the default when a match starts.
    SingleAttempt(FinishTimeout),
    /// Players restart as often as they like and their best time counts.
    /// The round ends after this many milliseconds. 0 behaves like a single attempt.
    TimeAttack(u32),
}

/// A warmup with the built-in warmup of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarmUp {
    /// Warmup rounds, 0 for none.
    pub rounds: u32,
    /// How long one warmup round lasts. 0 lets every player drive once.
    pub duration_ms: u32,
    /// Time to finish after the first player finished in rounds of one try.
    /// None computes it from the author time like the finish timeout.
    pub timeout_ms: Option<u32>,
}

/// How many laps a race has on maps with laps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Laps {
    /// The laps of the map. Every match starts with them.
    Map,
    /// This many laps.
    Count(u32),
    /// The race never finishes, e.g. for an endurance race in time attack rounds,
    /// which end with their time limit. On maps without laps the finish still
    /// ends the race.
    Infinite,
}

/// The rules of the races in the rounds that start from now on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceRules {
    pub laps: Laps,
    /// Players can give up their race. Without, a race only ends at the finish or
    /// with the round, respawning at a checkpoint is still possible. Every match
    /// starts with give ups.
    pub give_up: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanSpawn {
    pub account_id: String,
    pub can_spawn: bool,
}

/// What a mode can make happen on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Applies to the next round that starts. Send it in reply to the map start or
    /// a round end.
    SetRoundFormat(RoundFormat),
    /// The warmup before the first round of the map. Send it in reply to the map
    /// start, warmup races are no rounds and never reach the mode.
    SetWarmUp(WarmUp),
    /// Applies to the next round that starts, like the round format.
    SetRaceRules(RaceRules),
    /// Players that cannot spawn are out of the race (e.g. eliminated).
    /// The host leaves out permissions it already sent in this match.
    SetCanSpawn(CanSpawn),
    /// Ends the map once the current hold continues.
    EndMap,
    /// Ends the match once the current hold continues.
    EndMatch,
    /// Plays this map of the playlist next, counted from 0. Without it the host
    /// plays the playlist in order and starts every match with its first map.
    SetNextMap(u32),
    /// Readable by manialinks through `netread Text Net_Driver_State for Teams[0]`.
    /// The host publishes the standings itself, this is for data of a mode the
    /// standings can not hold. Which manialinks show it is up to the config, modes
    /// bring none.
    SetNetState(String),
    /// A message for everyone in the chat.
    ChatMessage(String),
}

/// What a mode returns for an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The standings after the event. The host keeps them for the next event.
    pub standings: Standings,
    /// Applied in order.
    pub commands: Vec<Command>,
}
