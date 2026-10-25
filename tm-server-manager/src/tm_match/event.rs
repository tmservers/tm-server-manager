use spacetimedb::{ReducerContext, Table, Uuid, table};
use tm_server_types::{base::RoundTime, config::TmMode, event::Event};

use crate::{
    competition::{connection::internal_graph_resolution_node_finished, node::NodeHandle},
    maps::{TabTmMap, tab_tm_map},
    tm_match::{
        MatchWrite,
        chat::{MatchChat, tab_match_chat},
        leaderboard::{
            MatchRoundPlayer, MatchRoundPlayerExt, tab_match_round_player,
            tab_match_round_player_ext,
        },
        replay::MatchReplayWrite,
        state::{MatchState, tab_match_state},
    },
    user::{UserRead, UserV1, UserWrite},
};

#[derive(Debug)]
#[table(accessor = tab_match_event)]
pub struct MatchEvent {
    pub(crate) event: Event,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u64,

    #[index(hash)]
    pub(crate) match_id: u32,
}

pub(crate) fn handle_match_event(
    ctx: &ReducerContext,
    mut state: MatchState,
    event: Event,
) -> Result<(), String> {
    match &event {
        // We use this to always insert participating players of the round in the leaderboard.
        Event::StartLine(start_line) if state.live_round() => {
            let account_id = Uuid::parse_str(&start_line.account_id).unwrap();
            let user_id = ctx.user_id_from_account(account_id);

            let round = state.get_round();

            // This is mainly for TimeAttack where we are at the start multiple times.
            if ctx
                .db
                .tab_match_round_player()
                .match_round_player()
                .filter((state.match_id, round, user_id))
                .count()
                == 0
            {
                let player = ctx
                    .db
                    .tab_match_round_player()
                    .try_insert(MatchRoundPlayer::new(state.match_id, user_id, round))?;
                ctx.db
                    .tab_match_round_player_ext()
                    .try_insert(MatchRoundPlayerExt::new(
                        player.id,
                        state.match_id,
                        user_id,
                        round,
                        start_line.time,
                    ))?;
            } else {
                let mut entry = ctx
                    .db
                    .tab_match_round_player_ext()
                    .match_round_player()
                    .filter((state.match_id, round, user_id))
                    .next()
                    .unwrap();

                entry.add_start_line(start_line.time);

                ctx.db.tab_match_round_player_ext().id().update(entry);
            }
        }
        Event::WayPoint(way_point) if state.live_round() => {
            let account_id = Uuid::parse_str(&way_point.account_id).unwrap();
            let user_id = ctx.user_id_from_account(account_id);

            let round = state.get_round();

            if let Some(mut entry) = ctx
                .db
                .tab_match_round_player_ext()
                .match_round_player()
                .filter((state.match_id, round, user_id))
                .next()
            {
                if way_point.is_end_race {
                    let mut round_player =
                        ctx.db.tab_match_round_player().id().find(entry.id).unwrap();

                    if round_player.get_time() > way_point.racetime as i32
                        || round_player.get_time() == 0
                    {
                        round_player.set_time(way_point.racetime as i32);
                        ctx.db.tab_match_round_player().id().update(round_player);
                    }

                    entry.add_finish(way_point.speed, way_point.racetime);
                } else if way_point.is_end_lap {
                    entry.add_lap(way_point.speed, way_point.racetime);
                } else {
                    entry.add_checkpoint(way_point.speed, way_point.racetime);
                }

                ctx.db.tab_match_round_player_ext().id().update(entry);
            } else {
                log::error!(
                    "Checkpoint without StartLine... Match: {}, Round: {}, Player: {}",
                    state.match_id,
                    round,
                    user_id
                );
            }
        }
        Event::Respawn(respawn) if state.live_round() => {
            let account_id = Uuid::parse_str(&respawn.account_id).unwrap();
            let user_id = ctx.user_id_from_account(account_id);

            let round = state.get_round();

            if let Some(mut entry) = ctx
                .db
                .tab_match_round_player_ext()
                .match_round_player()
                .filter((state.match_id, round, user_id))
                .next()
            {
                entry.add_respawn(respawn.speed, respawn.time);

                ctx.db.tab_match_round_player_ext().id().update(entry);
            } else {
                log::error!(
                    "Respawn without StartLine... Match: {}, Round: {}, Player: {}",
                    state.match_id,
                    round,
                    user_id
                );
            }
        }
        Event::GiveUp(give_up) if state.live_round() => {
            let account_id = Uuid::parse_str(&give_up.account_id).unwrap();
            let user_id = ctx.user_id_from_account(account_id);

            let round = state.get_round();

            if let Some(mut entry) = ctx
                .db
                .tab_match_round_player_ext()
                .match_round_player()
                .filter((state.match_id, round, user_id))
                .next()
            {
                entry.give_up(give_up.time);

                ctx.db.tab_match_round_player_ext().id().update(entry);
            } else {
                log::error!(
                    "GiveUp without StartLine... Match: {}, Round: {}, Player: {}",
                    state.match_id,
                    round,
                    user_id
                );
            }
        }
        Event::KnockoutElimination(knocked_players) if state.live_round() => {
            let round = state.get_round();

            for player in &knocked_players.account_ids {
                let account_id = Uuid::parse_str(player).unwrap();
                let user_id = ctx.user_id_from_account(account_id);

                let mut entry = ctx
                    .db
                    .tab_match_round_player()
                    .match_round_player()
                    .filter((state.match_id, round, user_id))
                    .next()
                    .unwrap_or_else(|| {
                        log::error!("Entry of player was not found.");
                        let new_player = MatchRoundPlayer::new(state.match_id, user_id, round);

                        ctx.db.tab_match_round_player().insert(new_player)
                    });

                entry.set_points(-1);

                ctx.db.tab_match_round_player().id().update(entry);
            }
        }
        Event::StartMapStart(start_map) => {
            let account_id = Uuid::parse_str(&start_map.map.author_account_id).unwrap();
            let user_id = if !ctx.has_user(account_id) {
                let mut user = UserV1::new(account_id);
                user.set_name(start_map.map.author_nickname.clone());

                ctx.user_insert(user).unwrap()
            } else {
                ctx.user_id_from_account(account_id)
            };

            let map = ctx
                .db
                .tab_tm_map()
                .uid()
                .find(&start_map.map.uid)
                .unwrap_or_else(|| {
                    //log::error!("Map uid could not be found for the StartMap callback. This should not be possible since matches have only known maps conifgured! Map: {}",start_map.map.uid);
                    ctx.db.tab_tm_map().insert(TabTmMap::new(
                        start_map.map.name.clone(),
                        start_map.map.uid.clone(),
                        user_id,
                        start_map.map.author_time,
                        start_map.map.gold_time,
                        start_map.map.silver_time,
                        start_map.map.bronze_time,
                    ))
                });
            state.set_map(map.id);

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::EndRoundStart(event) => {
            if state.live_round() {
                if let Err(err) = ctx.match_round_replay_time_update(
                    state.match_id,
                    event.time,
                    state.get_map(),
                    state.get_round(),
                    true,
                ) {
                    log::error!("Could not update match_round_replay_time. Reason: {err}");
                }
            } else if let Err(err) = ctx.match_round_replay_time_update(
                state.match_id,
                event.time,
                state.get_map(),
                state.get_round(),
                false,
            ) {
                log::error!("Could not update match_round_replay_time. Reason: {err}");
            }
        }
        Event::StartRoundStart(_) if state.live_round() => {
            state.new_round();

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::StartMatchStart(_) => {
            state.set_live_commited();
            ctx.db.tab_match_state().match_id().update(state);
            log::info!("Match {} has started!", state.match_id)
        }
        Event::EndMatchEnd(_) => {
            if state.get_round() == 0 {
                if state.get_mode() == TmMode::TimeAttack {
                    log::info!("Skipping end because of time attack mode.");
                } else {
                    log::info!("Match said it ended but we are on round 0 so it is probably wrong.")
                }
            } else {
                #[allow(clippy::collapsible_else_if)]
                if state.is_live_commited() {
                    ctx.match_end(state.match_id)?;

                    if let Err(error) = internal_graph_resolution_node_finished(
                        ctx,
                        NodeHandle::MatchV1(state.match_id),
                    ) {
                        log::error!("Graph resolution could not be completed. Error {error}")
                    };
                } else {
                    log::error!("Match said it ended but it was not yet commited.")
                }
            }
        }
        Event::WarmupStart => {
            state.set_wu(true);

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::WarmupEnd => {
            state.set_wu(false);

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::WarmupStartRound(_) => {
            state.new_wu_round();

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::Pause(pause) => {
            // If we just entered a pause we need to delete the current ongoing round.
            if pause.active && !state.paused() {
                ctx.db
                    .tab_match_round_player()
                    .match_round()
                    .delete((state.match_id, state.get_round()));
                ctx.db
                    .tab_match_round_player_ext()
                    .match_round()
                    .delete((state.match_id, state.get_round()));
            }

            state.set_pause(pause.active);
            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::Scores(scores) if state.live_round() => {
            if scores.section == "PreEndRound" {
                // Because we delete after a pause this is empty and hence does not modify anything.
                let player_rounds = ctx
                    .db
                    .tab_match_round_player()
                    .match_round()
                    .filter((state.match_id, state.get_round()));

                #[derive(Debug)]
                struct ScoresPlayer {
                    user_id: u32,
                    round_points: i32,
                    time: i32,
                    match_points: i32,
                    position: u16,
                }

                let mut scores = scores
                    .players
                    .iter()
                    .filter_map(|p| {
                        let user_id =
                            ctx.user_id_from_account(Uuid::parse_str(&p.account_id).unwrap());

                        if matches!(state.get_mode(), TmMode::ReverseCup) && p.match_points <= -2000
                        {
                            return None;
                        }

                        Some(ScoresPlayer {
                            user_id,
                            position: 0,
                            round_points: p.round_points,
                            match_points: p.match_points,
                            time: if let RoundTime::Time(time) = p.previous_racetime {
                                time as i32
                            } else {
                                -1
                            },
                        })
                    })
                    .collect::<Vec<_>>();

                // Reconstruct the position because the mode does not give it back.
                match state.get_mode() {
                    TmMode::Rounds => scores.sort_by_key(|k| -k.round_points),
                    TmMode::ReverseCup => scores.sort_by_key(|k| -k.round_points),
                    TmMode::Knockout => {
                        scores.sort_by_key(|k| if k.time <= 0 { i32::MAX } else { k.time })
                    }
                    TmMode::TimeAttack => {
                        scores.sort_by_key(|k| if k.time <= 0 { i32::MAX } else { k.time })
                    }
                    TmMode::Unknown => unreachable!(), // Since this a match we always have a mode available.
                }

                for (index, stand) in scores.iter_mut().enumerate() {
                    stand.position = (index + 1) as u16;
                }

                for mut player_round in player_rounds {
                    let found = scores.iter().find(|p| p.user_id == player_round.user_id);

                    if let Some(found) = found {
                        player_round.set_points(found.round_points);
                        player_round.set_position(found.position);
                        // Player is now in last chance
                        if matches!(state.get_mode(), TmMode::ReverseCup)
                            && found.match_points == -1000
                        {
                            player_round.set_points(found.match_points);
                        }

                        //player_round.set_position(found.position);
                        ctx.db.tab_match_round_player().id().update(player_round);
                    } else {
                        log::error!(
                            "Player of a round could not be found in the scores even tho he was on the start line..?"
                        )
                    };
                }
            }
            if scores.section == "EndRound" && state.get_mode() == TmMode::ReverseCup {
                // Because we delete after a pause this is empty and hence does not modify anything.
                let player_rounds = ctx
                    .db
                    .tab_match_round_player()
                    .match_round()
                    .filter((state.match_id, state.get_round()));

                #[derive(Debug)]
                struct ScoresPlayer {
                    user_id: u32,
                    round_points: i32,
                    time: i32,
                    match_points: i32,
                    position: u16,
                }

                let scores = scores
                    .players
                    .iter()
                    .map(|p| {
                        let user_id =
                            ctx.user_id_from_account(Uuid::parse_str(&p.account_id).unwrap());

                        ScoresPlayer {
                            user_id,
                            position: 0,
                            round_points: p.round_points,
                            match_points: p.match_points,
                            time: if let RoundTime::Time(time) = p.previous_racetime {
                                time as i32
                            } else {
                                -1
                            },
                        }
                    })
                    .collect::<Vec<_>>();

                for mut player_round in player_rounds {
                    let found = scores.iter().find(|p| p.user_id == player_round.user_id);

                    if let Some(found) = found {
                        // Player is now eliminated.
                        if matches!(state.get_mode(), TmMode::ReverseCup)
                            && found.match_points <= -2000
                        {
                            player_round.set_points(found.match_points);
                        }

                        ctx.db.tab_match_round_player().id().update(player_round);
                    } else {
                        log::error!(
                            "Player of a round could not be found in the scores even tho he was on the start line..?"
                        )
                    };
                }
            }
        }
        _ => (),
    }

    ctx.db.tab_match_event().try_insert(MatchEvent {
        match_id: state.match_id,
        event,
        id: 0,
    })?;

    Ok(())
}
