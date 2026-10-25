//! Conversions between the tm-server-types driver types and the generated WIT types.
//!
//! Every match is exhaustive without a catch-all arm. Adding a callback or a
//! command on either side fails to compile until it is mapped here.

use tm_server_types::{
    event::{DriverEvent, DriverPlayer, DriverRaceResult},
    method::{DriverCanSpawn, DriverCommand, DriverFinishTimeout, DriverLayer, DriverPlayerPoints},
};

use crate::tmservers::driver::types::{
    CanSpawn, Command, Event, FinishTimeout, Layer, MapStart, MatchStart, Player, PlayerJoin,
    PlayerLeave, PlayerPoints, RaceResult, RoundEnd, RoundStart,
};

/// Returns `None` for callbacks the host handles itself.
pub(crate) fn event(event: &DriverEvent) -> Option<Event> {
    let event = match event {
        // Only resets the session tracking of the host.
        DriverEvent::Hello(_) => return None,
        DriverEvent::MatchStart(e) => Event::MatchStart(MatchStart {
            time: e.time,
            players: players(&e.players),
        }),
        DriverEvent::MapStart(e) => Event::MapStart(MapStart {
            time: e.time,
            map_uid: e.map_uid.clone(),
            map_name: e.map_name.clone(),
            players: players(&e.players),
        }),
        DriverEvent::RoundStart(e) => Event::RoundStart(RoundStart {
            time: e.time,
            round: e.round,
            players: players(&e.players),
        }),
        DriverEvent::RoundEnd(e) => Event::RoundEnd(RoundEnd {
            time: e.time,
            round: e.round,
            forced: e.forced,
            results: e.results.iter().map(race_result).collect(),
        }),
        DriverEvent::MapEnd(e) => Event::MapEnd(e.time),
        DriverEvent::MatchEnd(e) => Event::MatchEnd(e.time),
        DriverEvent::PlayerJoin(e) => Event::PlayerJoin(PlayerJoin {
            time: e.time,
            player: player(&e.player),
        }),
        DriverEvent::PlayerLeave(e) => Event::PlayerLeave(PlayerLeave {
            time: e.time,
            account_id: e.account_id.clone(),
        }),
    };
    Some(event)
}

fn players(players: &[DriverPlayer]) -> Vec<Player> {
    players.iter().map(player).collect()
}

fn player(player: &DriverPlayer) -> Player {
    Player {
        account_id: player.account_id.clone(),
        name: player.name.clone(),
        team: player.team,
        spectator: player.spectator,
    }
}

fn race_result(result: &DriverRaceResult) -> RaceResult {
    RaceResult {
        account_id: result.account_id.clone(),
        time: result.finish_time(),
        checkpoints: result.checkpoints.clone(),
    }
}

impl From<Command> for DriverCommand {
    fn from(command: Command) -> Self {
        match command {
            Command::SetPlayerPoints(PlayerPoints {
                account_id,
                round_points,
                map_points,
                match_points,
            }) => DriverCommand::SetPlayerPoints(DriverPlayerPoints {
                account_id,
                round_points,
                map_points,
                match_points,
            }),
            Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn,
            }) => DriverCommand::SetCanSpawn(DriverCanSpawn {
                account_id,
                can_spawn,
            }),
            Command::SetFinishTimeout(timeout) => DriverCommand::SetFinishTimeout(match timeout {
                FinishTimeout::Automatic => DriverFinishTimeout::Automatic,
                FinishTimeout::Disabled => DriverFinishTimeout::Disabled,
                FinishTimeout::Milliseconds(ms) => DriverFinishTimeout::Milliseconds(ms),
            }),
            Command::ForceEndRound => DriverCommand::ForceEndRound,
            Command::EndMap => DriverCommand::EndMap,
            Command::EndMatch => DriverCommand::EndMatch,
            Command::SetMessage(message) => DriverCommand::SetMessage(message),
            Command::SetLayer(Layer { name, manialink }) => {
                DriverCommand::SetLayer(DriverLayer { name, manialink })
            }
            Command::RemoveLayer(name) => DriverCommand::RemoveLayer(name),
            Command::SetNetState(state) => DriverCommand::SetNetState(state),
        }
    }
}

#[cfg(test)]
mod tests {
    use tm_server_types::event::{DriverHello, DriverRoundEnd};

    use super::*;

    #[test]
    fn hello_stays_in_the_host() {
        let hello = DriverEvent::Hello(DriverHello {
            session: 1,
            seq: 1,
            time: 0,
            script_version: "0.1.0".into(),
        });
        assert!(event(&hello).is_none());
    }

    #[test]
    fn dnf_has_no_time() {
        let round_end = DriverEvent::RoundEnd(DriverRoundEnd {
            session: 1,
            seq: 4,
            time: 1000,
            round: 2,
            forced: false,
            results: vec![DriverRaceResult {
                account_id: "a".into(),
                time: -1,
                checkpoints: vec![],
            }],
        });
        let Some(Event::RoundEnd(round_end)) = event(&round_end) else {
            panic!("Expected a round end");
        };
        assert_eq!(round_end.results[0].time, None);
    }

    #[test]
    fn commands_keep_their_values() {
        let command = DriverCommand::from(Command::SetPlayerPoints(PlayerPoints {
            account_id: "a".into(),
            round_points: Some(10),
            map_points: None,
            match_points: Some(25),
        }));
        assert_eq!(
            command,
            DriverCommand::SetPlayerPoints(DriverPlayerPoints {
                account_id: "a".into(),
                round_points: Some(10),
                map_points: None,
                match_points: Some(25),
            })
        );
    }
}
