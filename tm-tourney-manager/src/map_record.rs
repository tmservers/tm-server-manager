use spacetimedb::{Timestamp, table};

use crate::{record::TmRecord, user::User};

#[table(
    name = tm_map_record,
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

impl TmMapRecord {
    pub(crate) fn with_player_info(self, player: User) -> TmRecord {
        TmRecord {
            map_uid: self.map_uid,
            player_uid: self.player_uid,
            timestamp: self.timestamp,
            time: self.time,
            //TODO
            ghost: "".into(),
            //TODO
            zone: "".into(),
            player_name: "".into(),
        }
    }

    pub(crate) fn player(&self) -> &String {
        &self.player_uid
    }
}
