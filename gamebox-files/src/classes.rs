mod game_ctn_challenge;
mod game_ctn_replay_record;

use std::marker::PhantomData;

pub use game_ctn_challenge::GameCtnChallenge;
pub use game_ctn_replay_record::GameCtnReplayRecord;

use crate::{GBXClass, GBXError};

/// This provides the main entry points for parsing classes.
/// It is called ChunkId because the ClassId is a subset of ChunkId
/// and can be retrieved from it.
#[derive(Debug)]
pub(crate) struct ChunkId(u32);
impl ChunkId {
    #[inline]
    pub fn new(id: u32) -> Self {
        ChunkId(id)
    }

    pub fn try_parse(&self, buffer: Vec<u8>) -> Result<GBXClass, GBXError> {
        match self.0 {
            // CGameCtnReplayRecord
            0x03093000 => match GameCtnReplayRecord::try_parse(buffer) {
                Ok(class) => Ok(GBXClass::GameCtnReplayRecord(class)),
                Err(error) => Err(error),
            },
            _ => Err(GBXError::UnknownChunk(self.0)),
        }
    }
}

/// Used to Lazily parse gamebox classes.
/// Stores the raw byte buffer and can be converted to the
/// underlying class if necessary.
#[derive(Debug)]
struct Proxy<T> {
    raw: Box<[u8]>,
    phantom: PhantomData<T>,
}

impl<T> Proxy<T> {
    fn new(buffer: Box<[u8]>) -> Proxy<T> {
        Self {
            raw: buffer,
            phantom: PhantomData,
        }
    }
}
