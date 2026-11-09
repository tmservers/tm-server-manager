//! Write modes for the mode driver of tm-server-manager.
//!
//! A mode is the rules of a match: what the points of a round are, who is out and
//! when maps and matches end. Implement [`Mode`] for one compiled into the driver.
//! Modes written as scripts get the same events and standings from their engine.
//!
//! The scores of a match live in [`Standings`] every mode shares. The host keeps
//! them and hands them to the mode with every event, so a mode keeps nothing but
//! its settings. The host also shows the standings to the players and reports
//! them to the manager, a mode only applies its rules to them. What the host
//! knows about the players and the match, e.g. names, comes from the [`Host`] a
//! mode gets with every event.
//!
//! Everything a mode knows comes from its settings, the standings, the
//! [`Event`]s and the [`Host`], which a replay of the match repeats exactly, so it
//! rebuilds the same standings. A mode must not depend on wall clock time,
//! randomness or the order of a hash map to stay replayable.
//!
//! Nobody ties: [`race_order`] tells equal races apart by the race times at the
//! last checkpoint, then at the one before and so on, and finally by the account.
//!
//! Settings can change during a match, the ones [`Mode::RELOADABLE`] lists. The
//! SDK rejects new settings that change any other one, and the mode gets
//! [`Event::SettingsChanged`] once it took them.
//!
//! While a round runs, the host shows it live with what [`Mode::live_round`]
//! places from the races so far. How it shows is up to the manialinks the config
//! selects, a mode brings none.

mod host;
mod instance;
mod races;
#[doc(hidden)]
pub mod reload;
pub mod settings;
mod standings;
pub mod testing;
mod types;

pub use host::{Host, LogLevel};
pub use instance::{ModeFault, ModeInstance, ModeSource, Native};
pub use races::{CountedRace, race_order};
pub use types::{
    CanSpawn, Command, Decision, Entry, Event, FinishTimeout, Label, Laps, Race, RaceRules, Racer,
    RoundEnd, RoundFormat, Standings, Status, Tone, User, WarmUp,
};

/// A game mode compiled into the driver. One instance lives for the whole match.
pub trait Mode: Sized + 'static {
    /// Shown above the standings, e.g. `Reverse Cup`.
    const NAME: &'static str;

    /// The settings of the match as json. Give the struct `#[serde(default)]` so
    /// every field is optional and `#[serde(deny_unknown_fields)]` so typos are
    /// rejected. It serializes too, so the SDK can tell what new settings change.
    type Settings: serde::de::DeserializeOwned + serde::Serialize;

    /// The settings that can change during a match, by their name in the json, e.g.
    /// a points repartition. New settings that change any other one are rejected,
    /// e.g. the lives players already hold. None by default.
    ///
    /// New settings apply from the next round on, the mode gets
    /// [`Event::SettingsChanged`] once it took them. What happened before stays,
    /// e.g. the points of past rounds.
    const RELOADABLE: &'static [&'static str] = &[];

    /// Creates the mode from the parsed settings of the match. Only validates them
    /// and derives what the mode needs, the parsing is [`Mode::from_json`].
    fn init(settings: Self::Settings) -> Result<Self, String>;

    /// Creates the mode from the json settings of the match. Empty settings are `{}`.
    fn from_json(settings: &str) -> Result<Self, String> {
        reload::Configured::<Self>::new(settings).map(|configured| configured.mode)
    }

    /// Applies the rules of the mode to an event and returns the commands for the
    /// server in order.
    ///
    /// The scores live in `standings`, which the host keeps between events. That is
    /// why a mode only gets `&self`: everything besides its settings belongs into the
    /// standings.
    fn step(&self, host: &dyn Host, standings: &mut Standings, event: Event) -> Vec<Command>;

    /// The round that runs as it stands, placed as if it ended now with the races so
    /// far, e.g. with the points finishers would get. The host shows it live. It
    /// gets the standings to read only: the round end still decides, see
    /// [`Standings::placed`]. Empty by default, so the round only shows once it ended.
    fn live_round(
        &self,
        _host: &dyn Host,
        _standings: &Standings,
        _races: &RoundEnd,
    ) -> Vec<Entry> {
        Vec::new()
    }
}

/// A place for chat messages: 1st, 2nd, 3rd, 4th, …, 11th, 12th, 13th, …, 21st, …
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinals() {
        let ordinals: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            ordinals,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
    }
}
