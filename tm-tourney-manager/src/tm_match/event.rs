use spacetimedb::{
    AnonymousViewContext, ReducerContext, SpacetimeType, Table, Uuid, rand::seq::index, table, view,
};
use tm_server_types::event::Event;

use crate::{
    competition::connection::{NodeKindHandle, internal_graph_resolution_node_finished},
    raw_server::tab_raw_server_occupation,
    tm_match::{
        leaderboard::{
            TabTmMatchRoundPlayer, TabTmMatchRoundPlayerExt, tab_tm_match_round_player,
            tab_tm_match_round_player_ext,
        },
        state::{TmMatchState, tab_tm_match_state, tab_tm_match_state__view},
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
                .try_insert(TabTmMatchRoundPlayerExt::new(
                    match_id,
                    internal_account_id,
                    round,
                ))?;

            ctx.db
                .tab_tm_match_round_player()
                .try_insert(TabTmMatchRoundPlayer::new(
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
        Event::Respawn(respawn) => {
            let account_id = Uuid::parse_str(&respawn.account_id).unwrap();
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

            entry.add_respawn(respawn.speed);

            ctx.db.tab_tm_match_round_player_ext().id().update(entry);
        }
        Event::GiveUp(give_up) => {
            let account_id = Uuid::parse_str(&give_up.account_id).unwrap();
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

            entry.give_up();

            ctx.db.tab_tm_match_round_player_ext().id().update(entry);
        }
        Event::StartMapStart(start_map) => {
            let mut state = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap();

            //TODO set correct active map.
            //state.set_map(start_map.map.uid);

            ctx.db.tab_tm_match_state().match_id().update(state);
        }
        Event::StartRoundStart(_) => {
            let mut state = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap();

            //TODO there could be some weirdities with paused or warmup
            state.new_round();

            ctx.db.tab_tm_match_state().match_id().update(state);
        }
        Event::EndMatchEnd(_) => {
            let Some(mut tm_match) = ctx.db.tab_tm_match().id().find(match_id) else {
                return Err("Match not found".into());
            };
            tm_match.end_match();
            let tm_match = ctx.db.tab_tm_match().id().update(tm_match);

            internal_graph_resolution_node_finished(
                ctx,
                //tm_match.get_comp_id(),
                NodeKindHandle::MatchV1(tm_match.id),
            )?;

            ctx.db
                .tab_raw_server_occupation()
                .match_id()
                .delete(match_id);
            log::info!("The match {match_id} has successfully ended!");
        }
        Event::WarmupStart => {
            let mut state = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap();

            state.set_wu(true);

            ctx.db.tab_tm_match_state().match_id().update(state);
        }
        Event::WarmupEnd => {
            let mut state = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap();

            state.set_wu(false);

            ctx.db.tab_tm_match_state().match_id().update(state);
        }
        Event::WarmupStartRound(_) => {
            let mut state = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap();

            state.new_wu_round();

            ctx.db.tab_tm_match_state().match_id().update(state);
        }
        Event::Pause(pause) => {
            let mut state = ctx
                .db
                .tab_tm_match_state()
                .match_id()
                .find(match_id)
                .unwrap();

            state.set_pause(pause.active);

            ctx.db.tab_tm_match_state().match_id().update(state);
        }
        _ => (),
    }

    ctx.db
        .tab_tm_match_event()
        .try_insert(TmMatchEvent { match_id, event })?;

    Ok(())
}
