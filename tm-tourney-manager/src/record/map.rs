use spacetimedb::{AnonymousViewContext, ReducerContext, Table, Timestamp, reducer, table, view};

use crate::{
    auth::Authorization,
    record::TmRecord,
    user::{User, user__view},
};

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
            let player = ctx.db.user().id().find(r.player()).unwrap();
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
    ctx.auth_server()?;

    for record in ctx
        .db
        .tm_map_record()
        .record_id()
        .filter((&map_uid, &player_uid))
    {
        if record.time > time {
            /* TODO update */
            return Ok(());
        }
    }
    ctx.db.tm_map_record().insert(TmMapRecord {
        map_uid,
        player_uid,
        timestamp: ctx.timestamp,
        time,
    });

    Ok(())
}
