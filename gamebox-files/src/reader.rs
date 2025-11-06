use core::num;

use crate::classes::ChunkId;

pub struct GameboxReader<'a> {
    buffer: &'a [u8],
    cursor: usize,
}

impl<'a> GameboxReader<'a> {
    pub fn new(buffer: &'a [u8]) -> GameboxReader<'a> {
        Self { buffer, cursor: 0 }
    }

    pub fn parse_u32(&mut self) -> u32 {
        let v = u32::from_le_bytes(
            self.buffer[self.cursor..self.cursor + 4]
                .try_into()
                .unwrap(),
        );
        self.cursor += 4;
        v
    }

    pub fn parse_u16(&mut self) -> u16 {
        let v = u16::from_le_bytes(
            self.buffer[self.cursor..self.cursor + 2]
                .try_into()
                .unwrap(),
        );
        self.cursor += 2;
        v
    }

    pub fn parse_chunk_id(&mut self) -> ChunkId {
        ChunkId::new(self.parse_u32())
    }
    pub fn skip(&mut self, number: usize) {
        self.cursor += number;
    }
}
