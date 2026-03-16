use spacetimedb::{
    AnonymousViewContext, ReducerContext, Table, Timestamp, Uuid, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    record::TmRecord,
    user::{UserV1, tab_user__view},
};

#[table(
    accessor = tm_map_record,
    index(
        accessor = record_id,
        btree(columns = [map_uid, account_id]))
    ,public)] //TODO make private
pub struct TmMapRecord {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    map_uid: String,
    account_id: Uuid,

    timestamp: Timestamp,

    time: u32,
}

impl TmMapRecord {
    pub(crate) fn with_player_info(self, player: UserV1) -> TmRecord {
        TmRecord {
            map_uid: self.map_uid,
            account_id: self.account_id,
            timestamp: self.timestamp,
            time: self.time,
            //TODO
            ghost: Uuid::NIL,
            //TODO
            zone: "".into(),
            player_name: "".into(),
        }
    }

    pub(crate) fn player(&self) -> Uuid {
        self.account_id
    }
}

//TODO we need a map_uid arg or so
#[view(accessor= map_record,public)]
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
    account_id: Uuid,
    time: u32,
) -> Result<(), String> {
    //TODO
    //ctx.auth_worker()?;

    if let Some(mut record) = ctx
        .db
        .tm_map_record()
        .record_id()
        .filter((&map_uid, &account_id))
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
        account_id,
        timestamp: ctx.timestamp,
        time,
    });

    Ok(())
}
