use spacetimedb::{AnonymousViewContext, Timestamp, Uuid, table, view};

use crate::record::TmRecord;

#[table(
    accessor = tm_match_record,
    index(
        accessor = record_id,
        btree(columns = [match_id,map_uid, account_id]))
    )]
pub struct TmCompRecord {
    match_id: u32,
    map_uid: String,
    account_id: Uuid,

    timestamp: Timestamp,

    time: u32,
}

// Maybe implement "local records" as a view
// This would be usefull since we could compute it through the server_event table
// and could save duplicating the data :thinking:
// A downside is computational overhead but maybe the AnonymousViewContext handles that for me.
// Would probably need to introduce the competition_id into the event table.
// maybe it could also replace the tournament_id since a competition already has it as a foreign key.

#[view(accessor= match_record,public)]
pub fn match_record(ctx: &AnonymousViewContext /* TODO: match_id arg */) -> Vec<TmRecord> {
    //TODO the problem is that even with a btree filtering for records is probably expensive.
    //Maybe it is better if we ammortize that cost by dupllicatiing the local_records data on event insertion
    /*     let iter1 = ctx.db.tm_match_record().time().filter(0u32..5u32);

    let iter2 = ctx.db.tm_match_record().test().filter(0u32..5u32); */
    //.filter()
    /* .map(|r| {
        let player = ctx.db.user().id().find(r.player()).unwrap();
        r.with_player_info(player)
    })
    .collect() */
    Vec::new()
}
