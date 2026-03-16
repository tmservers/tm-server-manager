use spacetimedb::{Uuid, table};

#[table(accessor= tab_match_round_replay,index(accessor=match_round,hash(columns=[match_id,round])))]
pub struct MatchRoundReplay {
    #[index(hash)]
    match_id: u32,
    round: u8,
    map_uid: Uuid,
    object_id: Uuid,
}

impl MatchRoundReplay {
    pub(crate) fn new(match_id: u32, round: u8, map_uid: Uuid, object_id: Uuid) -> Self {
        MatchRoundReplay {
            match_id,
            round,
            map_uid,
            object_id,
        }
    }
}
