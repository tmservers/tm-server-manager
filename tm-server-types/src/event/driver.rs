//! Driver specific events.
//!
//! The driver modescript reports the match flow through the standard callbacks
//! of the base script. It adds [`DriverHold`] because the standard protocol has no
//! way to make the script wait for a decision, and [`DriverGhost`] because it has no
//! way to get the ghost of a single race.

use crate::event::Event;

/// Sent by the driver modescript when it waits for the driver.
/// It continues once [`DriverCommand::Continue`](crate::method::DriverCommand::Continue)
/// with the same `seq` arrives and sends the hold again periodically until then.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverHold {
    /// Random number chosen when the modescript starts.
    /// A new session means the script (or the whole server) restarted.
    pub session: i32,
    /// Increases by one for every hold within a session.
    pub seq: u32,
    pub section: DriverSection,
    /// Only for [`DriverSection::RoundEnd`]: the round was forced to end or skipped
    /// (e.g. after a pause) and should not count.
    pub forced: bool,
}

impl DriverHold {
    pub const CALLBACK: &str = "Driver.Hold";
}

/// Where in the match flow the modescript waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverSection {
    /// Before the first round of a map. Rules for the map are set here.
    MapStart,
    /// After a round. The standard `Trackmania.Scores` callback with the section
    /// `PreEndRound` right before it carries the race times of the round.
    RoundEnd,
    /// After a map.
    MapEnd,
}

/// Sent by the driver modescript once it saved a finished race as a replay of its own.
///
/// The ghosts of a round arrive before its [`DriverSection::RoundEnd`] hold. They are
/// no input of the driver, so replaying the inputs of a match stays the same.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverGhost {
    /// Same as [`DriverHold::session`].
    pub session: i32,
    /// Rounds the modescript started in this session, warmups not counted.
    pub round: u32,
    pub map_uid: String,
    pub account_id: String,
    /// Race time in milliseconds.
    pub time: i32,
    /// The path the modescript gave `Replay_Save`.
    pub path: String,
}

impl DriverGhost {
    pub const CALLBACK: &str = "Driver.Ghost";
}

/// Points and positions the driver decided for the current round.
///
/// This is no callback of the server. Recording it in the manager comes with its
/// database integration, until then the runner of `tm-mode-driver` only logs it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRoundResult {
    pub results: Vec<DriverPlayerResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverPlayerResult {
    pub account_id: String,
    pub points: i32,
    /// Starts at 1.
    pub position: u32,
}

impl<'a> From<&'a Event> for &'a DriverHold {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::DriverHold(event) => event,
            _ => unreachable!(),
        }
    }
}

impl<'a> From<&'a Event> for &'a DriverGhost {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::DriverGhost(event) => event,
            _ => unreachable!(),
        }
    }
}

impl<'a> From<&'a Event> for &'a DriverRoundResult {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::DriverRoundResult(event) => event,
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_holds() {
        let event = Event::from_modescript(
            DriverHold::CALLBACK,
            r#"{"session":42,"seq":7,"section":"RoundEnd","forced":true}"#.to_string(),
        );
        let Ok(Some(Event::DriverHold(hold))) = event else {
            panic!("Expected a hold but got {event:?}");
        };
        assert_eq!(hold.session, 42);
        assert_eq!(hold.seq, 7);
        assert_eq!(hold.section, DriverSection::RoundEnd);
        assert!(hold.forced);
    }

    #[test]
    fn parses_ghosts() {
        let event = Event::from_modescript(
            DriverGhost::CALLBACK,
            r#"{"session":42,"round":3,"map_uid":"uid","account_id":"account","time":23450,"path":"Driver/42/3/account_100"}"#
                .to_string(),
        );
        let Ok(Some(Event::DriverGhost(ghost))) = event else {
            panic!("Expected a ghost but got {event:?}");
        };
        assert_eq!(
            ghost,
            DriverGhost {
                session: 42,
                round: 3,
                map_uid: "uid".to_string(),
                account_id: "account".to_string(),
                time: 23450,
                path: "Driver/42/3/account_100".to_string(),
            }
        );
    }

    #[test]
    fn rejects_unknown_sections() {
        assert!(
            Event::from_modescript(
                DriverHold::CALLBACK,
                r#"{"session":1,"seq":1,"section":"RoundStart","forced":false}"#.to_string(),
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_unknown_driver_callbacks() {
        assert!(Event::from_modescript("Driver.RoundEnd", "{}".to_string()).is_err());
    }
}
