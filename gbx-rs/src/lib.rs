//! # Disclaimer
//! [gbx-rs] is mainly the result of needing access to a replay file for the tm-tourney-manager project.
//! It is thus not feature complete to parse all flavours of .Gbx files.
//! However the general structure of the parser should be fairly trivial to extend for new gbx classes.
//! This can be done with a fork or as a contribution.
use std::fmt::Debug;

use bytes::{Buf, Bytes};
use reader::GameboxReader;
use thiserror::Error;

use crate::classes::{ChunkId, GameCtnReplayRecord};

mod classes;
mod reader;

#[derive(Debug)]
pub enum GBXClass {
    GameCtnReplayRecord(GameCtnReplayRecord),
}

#[derive(Debug, Error)]
pub enum GBXError {
    #[error("The GBX keyword needs to be the first thing encountered in a file!")]
    MissingMagic,
    #[error("the data for key `{0}` is not available")]
    UnsupportedVersion(u16),
    #[error("The Decompression of the body failed. Reason {0}")]
    DecompressionFailed(minilzo_rs::Error),
    #[error("The parsing for ChunkId: {0} failed. The ChunkId is unknown.")]
    UnknownChunk(u32),
}

pub fn try_parse_buffer(buffer: &[u8]) -> Result<GBXClass, GBXError> {
    if &buffer[0..3] != b"GBX" {
        return Err(GBXError::MissingMagic);
    }

    // A cursor with helper functions.
    let mut reader = GameboxReader::new(&buffer[3..]);

    // The GBX version. This tool only supports version 6 for now.
    let version = reader.parse_u16();

    if version != 6 {
        return Err(GBXError::UnsupportedVersion(version));
    }

    // The following 4 bytes are unused because version 6 is always compressed.
    reader.skip(4);

    let class_id = reader.parse_chunk_id();
    println!("{class_id:?}");

    let user_data_size = reader.parse_u32() as usize;
    println!("{user_data_size}");

    let num_header_chunks = reader.parse_u32();
    println!("{num_header_chunks}");

    let mut header_entries = Vec::with_capacity(num_header_chunks as usize);
    for num_entry in 0..num_header_chunks {
        let cur_buf_pos = (num_entry * 8 + 21) as usize;
        header_entries.push(HeaderEntry {
            chunk_id: ChunkId::new(u32::from_le_bytes(
                buffer[cur_buf_pos..cur_buf_pos + 4].try_into().unwrap(),
            )),
            chunk_size: ChunkSize(u32::from_le_bytes(
                buffer[cur_buf_pos + 4..cur_buf_pos + 8].try_into().unwrap(),
            )),
        });
    }
    println!("{header_entries:#?}");

    for header in header_entries {
        let header_entry = (num_header_chunks * 8 + 21) as usize;
        /* let chunk = header
        .chunk_id
        .try_parse(buffer[header_entry..header_entry + header.chunk_size.0 as usize].to_vec()); */
    }

    let num_nodes = u32::from_le_bytes(
        buffer[17 + user_data_size..21 + user_data_size]
            .try_into()
            .unwrap(),
    );
    println!("{num_nodes:#?}");

    let num_external_nodes = u32::from_le_bytes(
        buffer[21 + user_data_size..25 + user_data_size]
            .try_into()
            .unwrap(),
    );
    println!("{num_external_nodes:#?}");

    let uncompressed_size = u32::from_le_bytes(
        buffer[25 + user_data_size..29 + user_data_size]
            .try_into()
            .unwrap(),
    ) as usize;
    println!("{uncompressed_size:#?}");

    let compressed_size = u32::from_le_bytes(
        buffer[29 + user_data_size..33 + user_data_size]
            .try_into()
            .unwrap(),
    ) as usize;
    println!("{compressed_size:#?}");

    let lzo = minilzo_rs::LZO::init().unwrap();
    match lzo.decompress_safe(
        &buffer[33 + user_data_size..33 + user_data_size + compressed_size],
        uncompressed_size,
    ) {
        Ok(body) => class_id.try_parse(body),
        Err(err) => Err(GBXError::DecompressionFailed(err)),
    }
}

#[derive(Debug)]
struct HeaderEntry {
    chunk_id: ChunkId,
    chunk_size: ChunkSize,
}

impl HeaderEntry {
    /* fn try_parse(&self, buffer: Vec<u8>) -> Result<GBX, GBXError> {
        match self.chunk_id.0 {
            0x03093000 => {
                println!("CGameCtnReplayRecord");
                //GameCtnReplayRecord::try_parse()
            }
            _ => println!("{}", self.chunk_id.0),
        }
        Err(GBXError::MissingMagic)
    } */
}

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
