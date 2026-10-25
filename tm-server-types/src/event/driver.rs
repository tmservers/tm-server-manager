//! Callbacks of the generic driver modescript (`Driver.*`).
//!
//! The modescript has no mode logic. It reports what happens on the server and
//! waits on [`DriverEvent::is_decision`] callbacks until the driver acknowledges
//! them with [`DriverCommand::Ack`](crate::method::DriverCommand::Ack).

use crate::event::Event;

/// Every callback the driver modescript can send.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum DriverEvent {
    Hello(DriverHello),
    MatchStart(DriverMatchStart),
    MapStart(DriverMapStart),
    RoundStart(DriverRoundStart),
    RoundEnd(DriverRoundEnd),
    MapEnd(DriverMapEnd),
    MatchEnd(DriverMatchEnd),
    PlayerJoin(DriverPlayerJoin),
    PlayerLeave(DriverPlayerLeave),
}

impl DriverEvent {
    pub const HELLO: &str = "Driver.Hello";
    pub const MATCH_START: &str = "Driver.MatchStart";
    pub const MAP_START: &str = "Driver.MapStart";
    pub const ROUND_START: &str = "Driver.RoundStart";
    pub const ROUND_END: &str = "Driver.RoundEnd";
    pub const MAP_END: &str = "Driver.MapEnd";
    pub const MATCH_END: &str = "Driver.MatchEnd";
    pub const PLAYER_JOIN: &str = "Driver.PlayerJoin";
    pub const PLAYER_LEAVE: &str = "Driver.PlayerLeave";

    /// Name of the modescript callback.
    pub fn callback_name(&self) -> &'static str {
        match self {
            DriverEvent::Hello(_) => Self::HELLO,
            DriverEvent::MatchStart(_) => Self::MATCH_START,
            DriverEvent::MapStart(_) => Self::MAP_START,
            DriverEvent::RoundStart(_) => Self::ROUND_START,
            DriverEvent::RoundEnd(_) => Self::ROUND_END,
            DriverEvent::MapEnd(_) => Self::MAP_END,
            DriverEvent::MatchEnd(_) => Self::MATCH_END,
            DriverEvent::PlayerJoin(_) => Self::PLAYER_JOIN,
            DriverEvent::PlayerLeave(_) => Self::PLAYER_LEAVE,
        }
    }

    /// Random number chosen by the modescript when it starts.
    /// A new session means the script (or the whole server) restarted.
    pub fn session(&self) -> i32 {
        self.header().0
    }

    /// Increases by one for every callback within a session.
    pub fn seq(&self) -> u32 {
        self.header().1
    }

    /// Server time in milliseconds when the callback was sent.
    pub fn time(&self) -> i32 {
        self.header().2
    }

    /// Decision callbacks block the modescript until they are acknowledged.
    pub fn is_decision(&self) -> bool {
        match self {
            DriverEvent::MatchStart(_)
            | DriverEvent::MapStart(_)
            | DriverEvent::RoundStart(_)
            | DriverEvent::RoundEnd(_)
            | DriverEvent::MapEnd(_)
            | DriverEvent::MatchEnd(_) => true,
            DriverEvent::Hello(_) | DriverEvent::PlayerJoin(_) | DriverEvent::PlayerLeave(_) => {
                false
            }
        }
    }

    fn header(&self) -> (i32, u32, i32) {
        match self {
            DriverEvent::Hello(e) => (e.session, e.seq, e.time),
            DriverEvent::MatchStart(e) => (e.session, e.seq, e.time),
            DriverEvent::MapStart(e) => (e.session, e.seq, e.time),
            DriverEvent::RoundStart(e) => (e.session, e.seq, e.time),
            DriverEvent::RoundEnd(e) => (e.session, e.seq, e.time),
            DriverEvent::MapEnd(e) => (e.session, e.seq, e.time),
            DriverEvent::MatchEnd(e) => (e.session, e.seq, e.time),
            DriverEvent::PlayerJoin(e) => (e.session, e.seq, e.time),
            DriverEvent::PlayerLeave(e) => (e.session, e.seq, e.time),
        }
    }
}

/// A player as seen by the driver modescript.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverPlayer {
    pub account_id: String,
    pub name: String,
    /// 0 is blue and 1 is red. Only meaningful in team modes.
    pub team: i32,
    pub spectator: bool,
}

/// Sent once when the modescript starts.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverHello {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
    pub script_version: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverMatchStart {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
    pub players: Vec<DriverPlayer>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverMapStart {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
    pub map_uid: String,
    pub map_name: String,
    pub players: Vec<DriverPlayer>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRoundStart {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
    /// Starts at 1 on every map and counts every started round.
    pub round: u32,
    pub players: Vec<DriverPlayer>,
}

/// The result of one player in a round.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRaceResult {
    pub account_id: String,
    /// Best race time of the round in milliseconds. Zero or negative if the player did not finish.
    /// Single attempt rounds only have one race per player.
    pub time: i32,
    /// Checkpoint times of the best race in milliseconds since its start.
    pub checkpoints: Vec<u32>,
}

impl DriverRaceResult {
    pub fn finish_time(&self) -> Option<u32> {
        (self.time > 0).then_some(self.time as u32)
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverRoundEnd {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
    pub round: u32,
    /// The round was forced to end or skipped (e.g. after a pause) and should not count.
    pub forced: bool,
    pub results: Vec<DriverRaceResult>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverMapEnd {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverMatchEnd {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverPlayerJoin {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
    pub player: DriverPlayer,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct DriverPlayerLeave {
    pub session: i32,
    pub seq: u32,
    pub time: i32,
    pub account_id: String,
}

impl<'a> From<&'a Event> for &'a DriverEvent {
    #[inline]
    fn from(value: &'a Event) -> Self {
        match value {
            Event::Driver(event) => event,
            _ => unreachable!(),
        }
    }
}

/// Allows typed callbacks for every driver callback.
macro_rules! from_driver_event {
    ($($variant:ident => $ty:ty),* $(,)?) => {
        $(
            impl<'a> From<&'a Event> for &'a $ty {
                #[inline]
                fn from(value: &'a Event) -> Self {
                    match value {
                        Event::Driver(DriverEvent::$variant(event)) => event,
                        _ => unreachable!(),
                    }
                }
            }
        )*
    };
}

from_driver_event! {
    Hello => DriverHello,
    MatchStart => DriverMatchStart,
    MapStart => DriverMapStart,
    RoundStart => DriverRoundStart,
    RoundEnd => DriverRoundEnd,
    MapEnd => DriverMapEnd,
    MatchEnd => DriverMatchEnd,
    PlayerJoin => DriverPlayerJoin,
    PlayerLeave => DriverPlayerLeave,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(name: &str, body: &str) -> DriverEvent {
        match Event::from_modescript(name, body.to_string()) {
            Ok(Some(Event::Driver(event))) => event,
            other => panic!("Expected a driver event but got {other:?}"),
        }
    }

    #[test]
    fn parses_round_end() {
        let event = parse(
            DriverEvent::ROUND_END,
            r#"{"session":42,"seq":7,"time":123456,"round":3,"forced":false,"results":[
                {"account_id":"45b9cf1e-3c97-4753-ac63-ac61b48b4bb7","time":45210,"checkpoints":[10000,25000,45210]},
                {"account_id":"0a2d1bc0-4aaa-4374-b2db-3d561bdab1c9","time":-1,"checkpoints":[11000]}
            ]}"#,
        );

        assert_eq!(event.session(), 42);
        assert_eq!(event.seq(), 7);
        assert!(event.is_decision());
        let DriverEvent::RoundEnd(round_end) = event else {
            panic!("Wrong variant");
        };
        assert_eq!(round_end.round, 3);
        assert_eq!(round_end.results[0].finish_time(), Some(45210));
        assert_eq!(round_end.results[1].finish_time(), None);
    }

    #[test]
    fn parses_players() {
        let event = parse(
            DriverEvent::MAP_START,
            r#"{"session":1,"seq":2,"time":5,"map_uid":"olsKnq_qAghcVAnEkoeUnVHFZei","map_name":"Training - 01",
                "players":[{"account_id":"45b9cf1e-3c97-4753-ac63-ac61b48b4bb7","name":"kistz","team":0,"spectator":false}]}"#,
        );

        let DriverEvent::MapStart(map_start) = event else {
            panic!("Wrong variant");
        };
        assert_eq!(map_start.players[0].name, "kistz");
    }

    #[test]
    fn non_decision_callbacks() {
        let hello = parse(
            DriverEvent::HELLO,
            r#"{"session":1,"seq":1,"time":0,"script_version":"0.1.0"}"#,
        );
        assert!(!hello.is_decision());
        assert_eq!(hello.callback_name(), DriverEvent::HELLO);
    }

    #[test]
    fn rejects_unknown_driver_callbacks() {
        assert!(Event::from_modescript("Driver.DoesNotExist", "{}".to_string()).is_err());
    }

    #[test]
    fn rejects_malformed_bodies() {
        assert!(Event::from_modescript(DriverEvent::MAP_END, r#"{"seq":1}"#.to_string()).is_err());
    }
}
