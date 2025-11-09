use spacetimedb::{Timestamp, table};

#[table(
    name = tm_map_record,
    public,
    index(
        name = record_id,
        btree(columns = [map_uid, player_uid]))
    )]
pub struct TmMapRecord {
    map_uid: String,
    player_uid: String,

    timestamp: Timestamp,

    time: u32,
    // TODO
    // maybe compressed zone id?
    //maybe player name?
}
