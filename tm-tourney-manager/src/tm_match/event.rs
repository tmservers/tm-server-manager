use spacetimedb::{
    AnonymousViewContext, ReducerContext, SpacetimeType, Table, Uuid, rand::seq::index, table, view,
};
use tm_server_types::event::Event;

use crate::{
    competition::connection::{NodeKindHandle, internal_graph_resolution_node_finished},
    raw_server::tab_raw_server_occupation,
    tm_match::{
        leaderboard::{
            TmMatchRoundPlayer, TmMatchRoundPlayerExt, tab_tm_match_round_player,
            tab_tm_match_round_player_ext,
        },
        match_state::{TmMatchState, tab_tm_match_state, tab_tm_match_state__view},
        tab_tm_match,
    },
    user::tab_user,
};

#[derive(Debug)]
#[table(accessor = tab_tm_match_event)]
struct TmMatchEvent {
    #[index(hash)]
    pub(crate) match_id: u32,

    pub(crate) event: Event,
}

pub(crate) fn handle_match_event(
    ctx: &ReducerContext,
    match_id: u32,
    event: Event,
) -> Result<(), String> {
    match &event {
        // We use this to always insert participating players of the round in the leaderboard.
        Event::StartLine(start_line) => {
            let account_id = Uuid::parse_str(&start_line.account_id).unwrap();
            let internal_account_id = ctx
                .db
                .tab_user()
                .account_id()
                .find(account_id)
                .unwrap()
                .internal_id;

            let round = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap()
                .get_round();

            ctx.db
                .tab_tm_match_round_player_ext()
                .try_insert(TmMatchRoundPlayerExt::new(
                    match_id,
                    internal_account_id,
                    round,
                ))?;

            ctx.db
                .tab_tm_match_round_player()
                .try_insert(TmMatchRoundPlayer::new(
                    match_id,
                    internal_account_id,
                    round,
                ))?;
        }
        Event::WayPoint(way_point) => {
            let account_id = Uuid::parse_str(&way_point.account_id).unwrap();
            let internal_account_id = ctx
                .db
                .tab_user()
                .account_id()
                .find(account_id)
                .unwrap()
                .internal_id;

            let round = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap()
                .get_round();

            let mut entry = ctx
                .db
                .tab_tm_match_round_player_ext()
                .match_round_player()
                .filter((match_id, round, internal_account_id))
                .next()
                .unwrap();

            entry.add_checkpoint(way_point.speed, way_point.racetime);

            ctx.db.tab_tm_match_round_player_ext().id().update(entry);
        }
        Event::Respawn(respawn) => todo!(),
        Event::GiveUp(give_up) => todo!(),
        Event::LoadingMapStart(loading_map_start) => todo!(),
        Event::LoadingMapEnd(loading_map_end) => todo!(),
        Event::StartMapStart(start_map) => todo!(),
        Event::StartMapEnd(start_map) => todo!(),
        Event::EndMapStart(end_map_start) => todo!(),
        Event::EndMapEnd(end_map_end) => todo!(),
        Event::UnloadingMapStart(unloading_map_start) => todo!(),
        Event::UnloadingMapEnd(unloading_map_end) => todo!(),
        Event::StartTurnStart(start_turn) => todo!(),
        Event::StartTurnEnd(start_turn) => todo!(),
        Event::EndTurnStart(end_turn_start) => todo!(),
        Event::EndTurnEnd(end_turn_end) => todo!(),
        Event::StartRoundStart(start_round) => todo!(),
        Event::StartRoundEnd(start_round) => todo!(),
        Event::EndRoundStart(end_round_start) => todo!(),
        Event::EndRoundEnd(end_round_end) => todo!(),
        Event::PodiumStart(podium) => todo!(),
        Event::PodiumEnd(podium) => todo!(),
        Event::StartMatchStart(start_match) => todo!(),
        Event::StartMatchEnd(start_match) => todo!(),
        Event::EndMatchEnd(_) => {
            let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(match_id) else {
                return Err("Match not found".into());
            };
            tm_match.end_match();
            let tm_match = ctx.db.tab_tm_match().id().update(tm_match);

            internal_graph_resolution_node_finished(
                ctx,
                tm_match.get_comp_id(),
                NodeKindHandle::MatchV1(tm_match.id),
            )?;

            ctx.db
                .tab_raw_server_occupation()
                .match_id()
                .delete(match_id);
            log::info!("The match {match_id} has successfully ended!");
        }
        Event::WarmupStart => todo!(),
        Event::WarmupEnd => todo!(),
        Event::WarmupStartRound(warmup_round) => todo!(),
        Event::WarmupEndRound(warmup_round) => todo!(),
        _ => (),
    }

    ctx.db
        .tab_tm_match_event()
        .try_insert(TmMatchEvent { match_id, event })?;

    Ok(())
}
