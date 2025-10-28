use std::{fmt::Debug, io::Cursor};

use thiserror::Error;

#[derive(Debug)]
pub enum GBX {}

#[derive(Debug, Error)]
pub enum GBXError {
    #[error("The GBX keyword needs to be the first thing encountered in a file!")]
    MissingMagic,
    #[error("the data for key `{0}` is not available")]
    UnsupportedVersion(u16),
}

pub fn try_parse_buffer(buffer: Vec<u8>) -> Result<(), GBXError> {
    if &buffer[0..3] != b"GBX" {
        return Err(GBXError::MissingMagic);
    }
    // The GBX version. This tool only supports version 6.
    let version = u16::from_le_bytes(buffer[3..5].try_into().unwrap());

    if version != 6 {
        return Err(GBXError::UnsupportedVersion(version));
    }

    // The following 4 bytes are unused because version 6 is always compressed.

    let class_id = ClassId(u32::from_le_bytes(buffer[9..13].try_into().unwrap()));
    println!("{class_id:?}");

    let user_data_size = u32::from_le_bytes(buffer[13..17].try_into().unwrap());
    println!("{user_data_size}");

    let num_header_chunks = u32::from_le_bytes(buffer[17..21].try_into().unwrap());
    println!("{num_header_chunks}");

    let mut header_entries = Vec::with_capacity(num_header_chunks as usize);
    for num_entry in 0..num_header_chunks {
        let cur_buf_pos = (num_entry * 8 + 21) as usize;
        header_entries.push(HeaderEntry {
            chunk_id: ClassId(u32::from_le_bytes(
                buffer[cur_buf_pos..cur_buf_pos + 4].try_into().unwrap(),
            )),
            chunk_size: ChunkSize(u32::from_le_bytes(
                buffer[cur_buf_pos + 4..cur_buf_pos + 8].try_into().unwrap(),
            )),
        });
    }
    println!("{header_entries:#?}");

    for header in header_entries {}

    let num_nodes = u32::from_le_bytes(
        buffer[17 + (user_data_size as usize)..21 + (user_data_size as usize)]
            .try_into()
            .unwrap(),
    );
    println!("{num_nodes:#?}");

    let num_external_nodes = u32::from_le_bytes(
        buffer[21 + (user_data_size as usize)..24 + (user_data_size as usize)]
            .try_into()
            .unwrap(),
    );
    println!("{num_external_nodes:#?}");

    Ok(())
}

#[derive(Debug)]
struct HeaderEntry {
    chunk_id: ClassId,
    chunk_size: ChunkSize,
}

#[derive(Debug)]
struct ClassId(u32);

/* impl Debug for ClassId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ClassId")
            .field(&self.0.to_le_bytes())
            .finish()
    }
} */

#[derive(Debug)]
struct ChunkSize(u32);
impl ChunkSize {
    fn get_size(&self) -> u32 {
        if self.is_heavy() {
            self.0 ^ 0b1 // Toggle a bit
        } else {
            self.0
        }
    }

    fn is_heavy(&self) -> bool {
        self.0 & (1 << 31) != 0
    }
}
