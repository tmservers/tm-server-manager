use std::io::BufRead;

pub trait GameboxReader: BufRead {
    fn read_u32(&mut self) -> u32 {
        3
    }
}
