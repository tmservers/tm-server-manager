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
        self.cursor += 4;
        u32::from_le_bytes(
            self.buffer[self.cursor..self.cursor + 4]
                .try_into()
                .unwrap(),
        )
    }

    pub fn parse_u16(&mut self) -> u16 {
        self.cursor += 2;
        u16::from_le_bytes(
            self.buffer[self.cursor..self.cursor + 2]
                .try_into()
                .unwrap(),
        )
    }

    pub fn parse_chunk_id(&mut self) -> ChunkId {
        ChunkId::new(self.parse_u32())
    }
}
