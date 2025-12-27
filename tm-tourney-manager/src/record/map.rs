use spacetimedb::{AnonymousViewContext, ReducerContext, Table, Timestamp, reducer, table, view};

use crate::{
    auth::Authorization,
    record::TmRecord,
    user::{User, tab_user__view},
};

#[table(
    name = tm_map_record,
    index(
        name = record_id,
        btree(columns = [map_uid, player_uid]))
    ,public)] //TODO make private
pub struct TmMapRecord {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    map_uid: String,
    player_uid: String,

    timestamp: Timestamp,

    time: u32,
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

//TODO we need a map_uid arg or so
#[view(name= map_record,public)]
pub fn map_record(ctx: &AnonymousViewContext) -> Vec<TmRecord> {
    ctx.db
        .tm_map_record()
        .record_id()
        .filter("vjyNNUu997cC5PW8e3x7Y9RsAF0")
        .map(|r| {
            let player = ctx.db.tab_user().account_id().find(r.player()).unwrap();
            r.with_player_info(player)
        })
        .collect()
}

#[reducer]
pub fn post_record(
    ctx: &ReducerContext,
    map_uid: String,
    player_uid: String,
    time: u32,
) -> Result<(), String> {
    //TODO
    //ctx.auth_worker()?;

    if let Some(mut record) = ctx
        .db
        .tm_map_record()
        .record_id()
        .filter((&map_uid, &player_uid))
        .next()
    {
        if record.time > time {
            record.time = time;
            record.timestamp = ctx.timestamp;
            ctx.db.tm_map_record().id().update(record);
            return Ok(());
        } else {
            return Ok(());
        }
    }

    ctx.db.tm_map_record().insert(TmMapRecord {
        id: 0,
        map_uid,
        player_uid,
        timestamp: ctx.timestamp,
        time,
    });

    Ok(())
}
