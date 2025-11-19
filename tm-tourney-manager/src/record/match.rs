use spacetimedb::{AnonymousViewContext, Timestamp, table, view};

use crate::record::TmRecord;

#[table(
    name = tm_match_record,
    index(
        name = record_id,
        btree(columns = [match_id,map_uid, player_uid]))
    )]
pub struct TmCompRecord {
    match_id: u32,
    map_uid: String,
    player_uid: String,

    timestamp: Timestamp,

    time: u32,
}

// Maybe implement "local records" as a view
// This would be usefull since we could compute it through the server_event table
// and could save duplicating the data :thinking:
// A downside is computational overhead but maybe the AnonymousViewContext handles that for me.
// Would probably need to introduce the competition_id into the event table.
// maybe it could also replace the tournament_id since a competition already has it as a foreign key.

#[view(name= match_record,public)]
pub fn match_record(ctx: &AnonymousViewContext /* TODO: match_id arg */) -> Vec<TmRecord> {
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
