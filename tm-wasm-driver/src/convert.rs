//! Conversions between the tm-server-types and the generated WIT types.
//!
//! The commands of a mode are matched exhaustively without a catch-all arm.
//! Adding a command on either side fails to compile until it is mapped here.

use tm_server_types::{
    base::{Player, RoundTime},
    event::{DriverPlayerResult, DriverRoundResult, Scores},
    method::{DriverCanSpawn, DriverCommand, DriverFinishTimeout, DriverLayer, DriverRoundFormat},
};

use crate::{
    DriverReply,
    bindings::tmservers::driver::types::{
        CanSpawn, Command, FinishTimeout, Layer, PlayerResult, RaceResult, RoundFormat,
    },
};

/// Race times of every player of a scores callback.
pub(crate) fn race_results(scores: &Scores) -> Vec<RaceResult> {
    scores.players.iter().map(race_result).collect()
}

fn race_result(player: &Player) -> RaceResult {
    RaceResult {
        account_id: player.account_id.clone(),
        name: player.name.clone(),
        previous_time: race_time(&player.previous_racetime),
        previous_checkpoints: player.previous_race_checkpoints.clone(),
        best_time: race_time(&player.best_racetime),
        best_checkpoints: player.best_race_checkpoints.clone(),
    }
}

fn race_time(time: &RoundTime) -> Option<u32> {
    match time {
        RoundTime::Time(time) if *time > 0 => Some(*time),
        RoundTime::Time(_) | RoundTime::None => None,
    }
}

/// Splits the commands of a mode into commands for the server and the round result.
pub(crate) fn reply(commands: Vec<Command>) -> DriverReply {
    let mut reply = DriverReply::default();
    for command in commands {
        let command = match command {
            Command::SetRoundFormat(format) => DriverCommand::SetRoundFormat(round_format(format)),
            Command::SetCanSpawn(CanSpawn {
                account_id,
                can_spawn,
            }) => DriverCommand::SetCanSpawn(DriverCanSpawn {
                account_id,
                can_spawn,
            }),
            Command::EndMap => DriverCommand::EndMap,
            Command::EndMatch => DriverCommand::EndMatch,
            Command::SetLayer(Layer { name, manialink }) => {
                DriverCommand::SetLayer(DriverLayer { name, manialink })
            }
            Command::RemoveLayer(name) => DriverCommand::RemoveLayer(name),
            Command::SetNetState(state) => DriverCommand::SetNetState(state),
            Command::RoundResult(results) => {
                reply.round_result = Some(DriverRoundResult {
                    results: results.into_iter().map(player_result).collect(),
                });
                continue;
            }
        };
        reply.commands.push(command);
    }
    reply
}

fn player_result(result: PlayerResult) -> DriverPlayerResult {
    DriverPlayerResult {
        account_id: result.account_id,
        points: result.points,
        position: result.position,
    }
}

fn round_format(format: RoundFormat) -> DriverRoundFormat {
    match format {
        RoundFormat::SingleAttempt(timeout) => DriverRoundFormat::SingleAttempt(match timeout {
            FinishTimeout::Automatic => DriverFinishTimeout::Automatic,
            FinishTimeout::Disabled => DriverFinishTimeout::Disabled,
            FinishTimeout::Milliseconds(ms) => DriverFinishTimeout::Milliseconds(ms),
        }),
        RoundFormat::TimeAttack(time_limit) => DriverRoundFormat::TimeAttack(time_limit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_races_have_no_time() {
        assert_eq!(race_time(&RoundTime::None), None);
        assert_eq!(race_time(&RoundTime::Time(0)), None);
        assert_eq!(race_time(&RoundTime::Time(45_210)), Some(45_210));
    }

    #[test]
    fn round_results_stay_out_of_the_commands() {
        let reply = reply(vec![
            Command::RoundResult(vec![PlayerResult {
                account_id: "a".into(),
                points: 10,
                position: 1,
            }]),
            Command::EndMap,
        ]);

        assert_eq!(reply.commands, [DriverCommand::EndMap]);
        assert_eq!(
            reply.round_result,
            Some(DriverRoundResult {
                results: vec![DriverPlayerResult {
                    account_id: "a".into(),
                    points: 10,
                    position: 1,
                }]
            })
        );
    }

    #[test]
    fn round_formats_keep_their_values() {
        assert_eq!(
            round_format(RoundFormat::SingleAttempt(FinishTimeout::Milliseconds(
                7000
            ))),
            DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Milliseconds(7000))
        );
        assert_eq!(
            round_format(RoundFormat::TimeAttack(60_000)),
            DriverRoundFormat::TimeAttack(60_000)
        );
    }
}
