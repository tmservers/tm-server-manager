use spacetimedb::{Uuid, table};

#[table(accessor= tab_match_round_replay,index(accessor=match_round,hash(columns=[match_id,round])))]
struct MatchRoundReplay {
    map_id: u32,
    #[index(hash)]
    match_id: u32,
    round: u8,

    replay: Vec<u8>,
}

impl MatchRoundReplay {
    pub(crate) fn new(match_id: u32, round: u8, map_id: u32, replay: Vec<u8>) -> Self {
        MatchRoundReplay {
            match_id,
            round,
            map_id,
            replay,
        }
    }
}

pub(crate) trait MatchReplayRead {}
impl<Db: spacetimedb::DbContext> MatchReplayRead for Db {}

pub(crate) trait MatchReplayWrite: MatchReplayRead {
    fn insert_match_round_replay(
        &self,
        match_id: u32,
        round: u8,
        map_id: u32,
        replay: Vec<u8>,
    ) -> Result<(), String>;
}
impl<Db: spacetimedb::DbContext<DbView = spacetimedb::Local>> MatchReplayWrite for Db {
    fn insert_match_round_replay(
        &self,
        match_id: u32,
        round: u8,
        map_id: u32,
        replay: Vec<u8>,
    ) -> Result<(), String> {
        //self.db().tab_match_round_replay().match_id()
        todo!()
    }
}
