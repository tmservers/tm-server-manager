use spacetimedb::{Timestamp, table};

#[table(
    name = tm_comp_record,
    index(
        name = record_id,
        btree(columns = [competition_id,map_uid, player_uid]))
    )]
pub struct TmCompRecord {
    competition_id: u64,
    map_uid: String,
    player_uid: String,

    timestamp: Timestamp,

    time: u32,
}
