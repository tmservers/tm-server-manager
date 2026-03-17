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
        state::{MatchState, tab_match_state, tab_match_state__view},
        tab_match,
    },
    user::{tab_user, tab_user_ids_map},
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
                let internal_account_id = ctx
                    .db
                    .tab_user_ids_map()
                    .account_id()
                    .find(account_id)
                    .unwrap()
                    .user_id;

                let round = state.get_round();

                let player =
                    ctx.db
                        .tab_match_round_player()
                        .try_insert(TabMatchRoundPlayer::new(
                            match_id,
                            internal_account_id,
                            round,
                        ))?;
                ctx.db
                    .tab_match_round_player_ext()
                    .try_insert(TabMatchRoundPlayerExt::new(
                        player.id,
                        match_id,
                        internal_account_id,
                        round,
                    ))?;
            }
        }
        Event::WayPoint(way_point) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                let account_id = Uuid::parse_str(&way_point.account_id).unwrap();
                let internal_account_id = ctx
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
                    .filter((match_id, round, internal_account_id))
                    .next()
                    .unwrap();

                entry.add_checkpoint(way_point.speed, way_point.racetime);

                ctx.db.tab_match_round_player_ext().id().update(entry);
            }
        }
        Event::Respawn(respawn) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                let account_id = Uuid::parse_str(&respawn.account_id).unwrap();
                let internal_account_id = ctx
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
                    .filter((match_id, round, internal_account_id))
                    .next()
                    .unwrap();

                entry.add_respawn(respawn.speed);

                ctx.db.tab_match_round_player_ext().id().update(entry);
            }
        }
        Event::GiveUp(give_up) => {
            let state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();
            if state.live_round() {
                let account_id = Uuid::parse_str(&give_up.account_id).unwrap();
                let internal_account_id = ctx
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
                    .filter((match_id, round, internal_account_id))
                    .next()
                    .unwrap();

                entry.give_up();

                ctx.db.tab_match_round_player_ext().id().update(entry);
            }
        }
        Event::StartMapStart(start_map) => {
            let mut state = ctx.db.tab_match_state().match_id().find(match_id).unwrap();

            let account_id = Uuid::parse_str(&start_map.map.author_account_id).unwrap();
            let user_id = ctx
                .db
                .tab_user_ids_map()
                .account_id()
                .find(account_id)
                .unwrap()
                .user_id;

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
        Event::EndMatchEnd(_) => {
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
        _ => (),
    }

    ctx.db.tab_match_event().try_insert(MatchEvent {
        match_id,
        event,
        id: 0,
    })?;

    Ok(())
}
