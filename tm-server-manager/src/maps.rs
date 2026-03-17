use spacetimedb::{Uuid, table};

#[table(accessor=tab_tm_map)]
pub struct TabTmMap {
    name: String,
    uid: String,
    #[primary_key]
    id: Uuid,
    author_user_id: u32,
    author_time: u32,
    gold_time: u32,
    silver_time: u32,
    bronze_time: u32,
}
