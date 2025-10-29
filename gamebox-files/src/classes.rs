mod game_ctn_challenge;
mod game_ctn_replay_record;

use std::marker::PhantomData;

pub use game_ctn_challenge::GameCtnChallenge;
pub use game_ctn_replay_record::GameCtnReplayRecord;

use crate::{GBX, GBXError};

#[derive(Debug)]
pub(crate) struct ClassId(u32);
impl ClassId {
    #[inline]
    pub fn new(id: u32) -> Self {
        ClassId(id)
    }

    pub fn try_parse(&self, buffer: Vec<u8>) -> Result<GBX, GBXError> {
        match self.0 {
            // CGameCtnReplayRecord
            0x03093000 => match GameCtnReplayRecord::try_parse(buffer) {
                Ok(class) => Ok(GBX::GameCtnReplayRecord(class)),
                Err(error) => Err(error),
            },
            //TODO this is not the right error
            _ => Err(GBXError::MissingMagic),
        }
    }
}

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
