use spacetimedb::{Table, table};

#[table(accessor=tab_tm_map, vis_private)]
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

pub(crate) trait TmMapRead {
    fn tm_map_find_uid(&self, uid: &String) -> Option<TabTmMap>;
    fn tm_map_find(&self, map_id: u32) -> Option<TabTmMap>;
}
impl<Db: spacetimedb::CtxDbRead> TmMapRead for Db {
    fn tm_map_find_uid(&self, uid: &String) -> Option<TabTmMap> {
        self.db_read_only().tab_tm_map().uid().find(uid)
    }

    fn tm_map_find(&self, map_id: u32) -> Option<TabTmMap> {
        self.db_read_only().tab_tm_map().id().find(map_id)
    }
}

pub(crate) trait TmMapWrite: TmMapRead {
    fn tm_map_insert(&self, map: TabTmMap) -> Result<TabTmMap, String>;
}
impl<Db: spacetimedb::CtxDbWrite> TmMapWrite for Db {
    fn tm_map_insert(&self, map: TabTmMap) -> Result<TabTmMap, String> {
        Ok(self.db().tab_tm_map().try_insert(map)?)
    }
}
