use spacetimedb::{
    AnonymousViewContext, ReducerContext, SpacetimeType, Table, Uuid, rand::seq::index, table, view,
};
use tm_server_types::event::Event;

use crate::{
    competition::{connection::internal_graph_resolution_node_finished, node::NodeKindHandle},
    maps::{TabTmMap, tab_tm_map},
    raw_server::tab_raw_server_occupation,
    tm_match::{
        leaderboard::{
            TabMatchRoundPlayer, TabMatchRoundPlayerExt, tab_match_round_player,
            tab_match_round_player_ext,
        },
        state::tab_match_state,
        tab_match,
    },
    user::{UserIdsMap, UserV1, tab_user, tab_user_ids_map},
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
    match_id: u32,
    event: Event,
) -> Result<(), String> {
    match &event {
        // We use this to always insert participating players of the round in the leaderboard.
        Event::StartLine(start_line) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                let account_id = Uuid::parse_str(&start_line.account_id).unwrap();
                let user_id = ctx
                    .db
                    .tab_user_ids_map()
                    .account_id()
                    .find(account_id)
                    .unwrap()
                    .user_id;

                let round = state.get_round();

                let player = ctx
                    .db
                    .tab_match_round_player()
                    .try_insert(TabMatchRoundPlayer::new(match_id, user_id, round))?;
                ctx.db
                    .tab_match_round_player_ext()
                    .try_insert(TabMatchRoundPlayerExt::new(
                        player.id, match_id, user_id, round,
                    ))?;
            }
        }
        Event::WayPoint(way_point) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                let account_id = Uuid::parse_str(&way_point.account_id).unwrap();
                let user_id = ctx
                    .db
                    .tab_user_ids_map()
                    .account_id()
                    .find(account_id)
                    .unwrap()
                    .user_id;

                let round = state.get_round();

                let mut entry = ctx
                    .db
                    .tab_match_round_player_ext()
                    .match_round_player()
                    .filter((match_id, round, user_id))
                    .next()
                    .unwrap();

                entry.add_checkpoint(way_point.speed, way_point.racetime);

                if way_point.is_end_race {
                    let mut round_player =
                        ctx.db.tab_match_round_player().id().find(entry.id).unwrap();
                    round_player.set_time(way_point.racetime as i32);
                    ctx.db.tab_match_round_player().id().update(round_player);
                }

                ctx.db.tab_match_round_player_ext().id().update(entry);
            }
        }
        Event::Respawn(respawn) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                let account_id = Uuid::parse_str(&respawn.account_id).unwrap();
                let user_id = ctx
                    .db
                    .tab_user_ids_map()
                    .account_id()
                    .find(account_id)
                    .unwrap()
                    .user_id;

                let round = state.get_round();

                let mut entry = ctx
                    .db
                    .tab_match_round_player_ext()
                    .match_round_player()
                    .filter((match_id, round, user_id))
                    .next()
                    .unwrap();

                entry.add_respawn(respawn.speed, respawn.time);

                ctx.db.tab_match_round_player_ext().id().update(entry);
            }
        }
        Event::GiveUp(give_up) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                let account_id = Uuid::parse_str(&give_up.account_id).unwrap();
                let user_id = ctx
                    .db
                    .tab_user_ids_map()
                    .account_id()
                    .find(account_id)
                    .unwrap()
                    .user_id;

                let round = state.get_round();

                let mut entry = ctx
                    .db
                    .tab_match_round_player_ext()
                    .match_round_player()
                    .filter((match_id, round, user_id))
                    .next()
                    .unwrap();

                entry.give_up(give_up.time);

                ctx.db.tab_match_round_player_ext().id().update(entry);
            }
        }
        Event::StartMapStart(start_map) => {
            let mut state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();

            let account_id = Uuid::parse_str(&start_map.map.author_account_id).unwrap();
            let account = ctx
                .db
                .tab_user_ids_map()
                .account_id()
                .find(account_id)
                .unwrap_or_else(|| {
                    let user = ctx.db.tab_user().account_id().insert_or_update(UserV1::new(
                        account_id,
                        start_map.map.author_nickname.clone(),
                    ));
                    ctx.db
                        .tab_user_ids_map()
                        .account_id()
                        .insert_or_update(UserIdsMap::new(account_id, user.internal_id))
                });
            let user_id = account.user_id;

            let map = ctx
                .db
                .tab_tm_map()
                .uid()
                .find(&start_map.map.uid)
                .unwrap_or_else(|| {
                    log::error!("Map uid could not be found for the StartMap callback. This should not be possible since matches have only known maps conifgured! Map: {}",start_map.map.uid);
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
        Event::StartRoundStart(_) => {
            let mut state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                state.new_round();

                ctx.db.tab_match_state().match_id().update(state);
            }
        }
        Event::StartMatchStart(_) => {
            log::info!("Match {match_id} has started!")
        }
        Event::EndMatchEnd(_) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.get_round() == 0 {
                log::info!("Match said it ended but we are on round 0 so it is probably wrong.")
            }

            let Some(mut tm_match) = ctx.db.tab_match().id().find(match_id) else {
                return Err("Match not found".into());
            };
            tm_match.end_match();
            let tm_match = ctx.db.tab_match().id().update(tm_match);

            internal_graph_resolution_node_finished(ctx, NodeKindHandle::MatchV1(tm_match.id))?;

            ctx.db
                .tab_raw_server_occupation()
                .match_id()
                .delete(match_id);
            log::info!("The match {match_id} has successfully ended!");
        }
        Event::WarmupStart => {
            let mut state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();

            state.set_wu(true);

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::WarmupEnd => {
            let mut state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();

            state.set_wu(false);

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::WarmupStartRound(_) => {
            let mut state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();

            state.new_wu_round();

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::Pause(pause) => {
            let mut state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();

            state.set_pause(pause.active);

            ctx.db.tab_match_state().match_id().update(state);
        }
        Event::Scores(scores) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() && scores.section == "PreEndRound" {
                let player_rounds = ctx
                    .db
                    .tab_match_round_player()
                    .match_round()
                    .filter((match_id, state.get_round()));

                #[derive(Debug)]
                struct ScoresPlayer {
                    user_id: u32,
                    round_points: i32,
                }

                let scores = scores
                    .players
                    .iter()
                    .map(|p| {
                        let user = ctx
                            .db
                            .tab_user_ids_map()
                            .account_id()
                            .find(Uuid::parse_str(&p.account_id).unwrap())
                            .unwrap();
                        ScoresPlayer {
                            user_id: user.user_id,
                            round_points: p.round_points,
                        }
                    })
                    .collect::<Vec<_>>();

                log::error!("{:?}", scores);

                for mut player_round in player_rounds {
                    log::error!("{:?}", player_round);
                    let found = scores.iter().find(|p| p.user_id == player_round.user_id);

                    if let Some(found) = found {
                        player_round.set_points(found.round_points);
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
        match_id,
        event,
        id: 0,
    })?;

    Ok(())
}
