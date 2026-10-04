use spacetimedb::{CtxDbRead, ProcedureContext, Table, procedure, table};

#[table(accessor= tab_match_round_replay,index(accessor=match_round,hash(columns=[match_id,round])))]
pub struct MatchRoundReplay {
    replay: Vec<u8>,

    #[index(hash)]
    map_id: u32,
    #[index(hash)]
    match_id: u32,
    round: u16,
}

/* impl MatchRoundReplay {
    pub(crate) fn new(match_id: u32, round: u16, map_id: u32, replay: Vec<u8>) -> Self {
        MatchRoundReplay {
            match_id,
            round,
            map_id,
            replay,
        }
    }
} */

// Small helper table associating the correct round and timestamp to replay upload.
// This is necessary in case the MatchState tables increments the rounds value before
// the post_replay could be processed on the server.
// With this we verify the correct server timestamp for the replay.
#[table(accessor= tab_match_round_replay_time)]
struct MatchRoundReplayTime {
    #[primary_key]
    match_id: u32,

    ending_round_timestamp: u32,
    map_id: u32,
    ending_round: u16,
    saving_enabled: bool,
}

pub(crate) trait MatchReplayRead {}
impl<Db: spacetimedb::CtxDbRead> MatchReplayRead for Db {}

pub(crate) trait MatchReplayWrite: MatchReplayRead {
    fn match_round_replay_time_update(
        &self,
        match_id: u32,
        time: u32,
        map_id: u32,
        round: u16,
        saving_enabled: bool,
    ) -> Result<(), String>;

    fn insert_match_round_replay(
        &self,
        match_id: u32,
        ending_round_timestamp: u32,
        replay: Vec<u8>,
    ) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> MatchReplayWrite for Db {
    fn insert_match_round_replay(
        &self,
        match_id: u32,
        ending_round_timestamp: u32,
        replay: Vec<u8>,
    ) -> Result<(), String> {
        let Some(entry) = self
            .db()
            .tab_match_round_replay_time()
            .match_id()
            .find(match_id)
        else {
            return Err("Could not find Time".into());
        };
        if !entry.saving_enabled {
            return Err("Tried to save replay in a round where saving is disabled".into());
        }

        if ending_round_timestamp != entry.ending_round_timestamp {
            return Err("Timestamps did not match for that round.".into());
        }

        self.db()
            .tab_match_round_replay()
            .try_insert(MatchRoundReplay {
                map_id: entry.map_id,
                match_id,
                round: entry.ending_round,
                replay,
            })?;

        Ok(())
    }

    fn match_round_replay_time_update(
        &self,
        match_id: u32,
        time: u32,
        map_id: u32,
        round: u16,
        saving_enabled: bool,
    ) -> Result<(), String> {
        if let Some(mut entry) = self
            .db()
            .tab_match_round_replay_time()
            .match_id()
            .find(match_id)
        {
            entry.ending_round = round;
            entry.ending_round_timestamp = time;
            entry.map_id = map_id;
            entry.saving_enabled = saving_enabled;

            self.db()
                .tab_match_round_replay_time()
                .match_id()
                .update(entry);
        } else {
            self.db()
                .tab_match_round_replay_time()
                .try_insert(MatchRoundReplayTime {
                    match_id,
                    ending_round_timestamp: time,
                    map_id,
                    ending_round: round,
                    saving_enabled,
                })?;
        }

        Ok(())
    }
}

#[procedure]
fn match_round_replay(
    ctx: &mut ProcedureContext,
    match_id: u32,
    round: u16,
) -> Result<Vec<u8>, String> {
    ctx.try_with_tx(|ctx| {
        let Some(replay) = ctx
            .db_read_only()
            .tab_match_round_replay()
            .match_round()
            .filter((match_id, round))
            .next()
        else {
            return Err("Round of Match could not be found.".into());
        };
        Ok(replay.replay)
    })
}
