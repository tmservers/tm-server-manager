use spacetimedb::{AnonymousViewContext, SpacetimeType, Timestamp, view};

use crate::{record::map::tm_map_record__view, user::user__view};

mod local;
mod map;
/// General purpose Record Type used to query all sorts of leaderboards.
#[derive(Debug, SpacetimeType)]
pub struct TmRecord {
    pub map_uid: String,
    pub player_uid: String,

    pub timestamp: Timestamp,

    pub time: u32,

    pub zone: String,
    pub player_name: String,

    //TODO: figure this out
    pub ghost: String,
}

//TODO we need a map_uid arg or so
#[view(name= map_record,public)]
pub fn map_record(ctx: &AnonymousViewContext) -> Vec<TmRecord> {
    ctx.db
        .tm_map_record()
        .record_id()
        .filter(("huh", "huh"))
        .map(|r| {
            let player = ctx.db.user().id().find(r.player()).unwrap();
            r.with_player_info(player)
        })
        .collect()
}

// Maybe implement "local records" as a view
// This would be usefull since we could compute it through the server_event table
// and could save duplicating the data :thinking:
// A downside is computational overhead but maybe the AnonymousViewContext handles that for me.
// Would probably need to introduce the competition_id into the event table.
// maybe it could also replace the tournament_id since a competition already has it as a foreign key.

#[view(name= local_record,public)]
pub fn local_record(ctx: &AnonymousViewContext, /* TODO: competition_id arg */) -> Vec<TmRecord> {
    //TODO the problem is that even with a btree filtering for records is probably expensive.
    //Maybe it is better if we ammortize that cost by dupllicatiing the local_records data on event insertion
    /*  ctx.db
    .tm_server_event()
    .event()
    .filter()
    .map(|r| {
        let player = ctx.db.user().id().find(r.player()).unwrap();
        r.with_player_info(player)
    })
    .collect() */
    Vec::new()
}
