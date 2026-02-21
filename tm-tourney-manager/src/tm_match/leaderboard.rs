use spacetimedb::{AnonymousViewContext, SpacetimeType, Uuid, table, view};

#[table(accessor= tab_tm_match_leaderboard,index(accessor=round_id, hash(columns=[match_id,round])))]
pub struct TmMatchLeaderboard {
    account_id: Uuid,

    match_id: u32,
    round: u32,

    points: i32,

    #[auto_inc]
    #[primary_key]
    id: u32,
}

#[table(accessor= tab_tm_match_leaderboard_ext)]
pub struct TmMatchLeaderboardExt {
    account_id: Uuid,

    checkpoints: Vec<u32>,
}

#[view(accessor=match_leaderbaord,public)]
pub fn match_leaderboard(
    ctx: &AnonymousViewContext, /* match_id: u32 */
) -> Vec<TmMatchLeaderboard> {
    let match_id = 1u32;

    Vec::new()
}

/// If round 0 is supplied we take the current round.
#[view(accessor=match_round,public)]
pub fn match_leaderboard_live_round(
    ctx: &AnonymousViewContext, /* match_id: u32 */
) -> Vec<TmMatchLeaderboardExt> {
    let match_id = 1u32;
    let mut round = 0u16;

    /* if round == 0 {
        let Some(state) = ctx.db.tab_tm_match_state().id().find(match_id) else {
            return Vec::new();
        };
        round = state.round;
    }

    //let entries = Vec::with_capacity(8);
    //let players = Hash
    for event in ctx
        .db
        .tab_tm_match_event()
        .match_round_wu()
        .filter((match_id, round, false))
    {
        log::error!("{event:?}");

        match event.event {
            //Event::StartLine()
            _ => continue,
        }
    } */
    Vec::new()
}
