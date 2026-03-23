use spacetimedb::table;

#[table(accessor=tab_tm_map)]
pub struct TabTmMap {
    name: String,
    #[unique]
    pub uid: String,
    #[auto_inc]
    #[primary_key]
    pub id: u32,
    author_user_id: u32,
    author_time: u32,
    gold_time: u32,
    silver_time: u32,
    bronze_time: u32,
    internal: bool,
}

impl TabTmMap {
    pub fn new(
        name: String,
        uid: String,
        author_user_id: u32,
        author_time: u32,
        gold_time: u32,
        silver_time: u32,
        bronze_time: u32,
    ) -> Self {
        Self {
            name,
            uid,
            id: 0,
            author_user_id,
            author_time,
            gold_time,
            silver_time,
            bronze_time,
            //TODO
            internal: false,
        }
    }
}
