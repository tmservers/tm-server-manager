use crate::{
    GBXError,
    classes::{ChunkId, GameCtnChallenge, Proxy},
};

#[derive(Debug)]
pub struct GameCtnReplayRecord {
    version: u32,

    time: u32,

    driver_login: String,

    map: Proxy<GameCtnChallenge>,
}

impl GameCtnReplayRecord {
    pub fn try_parse(buffer: Vec<u8>) -> Result<Self, GBXError> {
        let chunk_id = ChunkId::new(u32::from_le_bytes(buffer[0..4].try_into().unwrap()));
        println!("ChunkId: {chunk_id:#?}");

        if chunk_id.0 == 0x03093002 {
            println!("yay")
        }
        /*
                uint32 version
        uint32 authorVersion
        string authorLogin
        string authorNick
        string authorZone
        string authorExtraInfo */

        /*  let version: u32 = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
        println!("Version: {version:#?}");

        let author_version: u32 = u32::from_le_bytes(buffer[8..12].try_into().unwrap());
        println!("Author Version: {author_version:#?}"); */

        let length = u32::from_le_bytes(buffer[4..8].try_into().unwrap()) as usize;
        println!("Length: {length:#?}");
        let map = Proxy::<GameCtnChallenge>::new(buffer[8..8 + length].into());

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[8 + length..12 + length].try_into().unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}");
        if chunk_id.0 == 0x03093014 {
            println!("yay")
        }

        //let author_login = String::from_utf8_lossy(&);
        let ignored: u32 = u32::from_le_bytes(buffer[12 + length..16 + length].try_into().unwrap());
        println!("Ignored: {ignored:#?}");

        let num_ghost: u32 =
            u32::from_le_bytes(buffer[16 + length..20 + length].try_into().unwrap());
        println!("Number of Ghosts: {num_ghost:#?}");

        let noderef = i32::from_le_bytes(buffer[20 + length..24 + length].try_into().unwrap());
        println!("Vesrion: {noderef:#?}");

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[24 + length..28 + length].try_into().unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}");
        if chunk_id.0 == 0x03092000 {
            println!("yay")
        }

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[28 + length..32 + length].try_into().unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}");
        if chunk_id.0 == 0x0303F006 {
            println!("yay")
        }

        let is_replaying = i32::from_le_bytes(buffer[32 + length..36 + length].try_into().unwrap());
        println!("Isreplaying: {is_replaying:#?}");

        let uncompressed_size =
            i32::from_le_bytes(buffer[36 + length..40 + length].try_into().unwrap()) as usize;
        println!("Uncommp: {uncompressed_size:#?}");

        let compressed_size =
            i32::from_le_bytes(buffer[40 + length..44 + length].try_into().unwrap()) as usize;
        println!("comp: {compressed_size:#?}");

        let mut body = Vec::with_capacity(uncompressed_size);
        let mut decompressor = flate2::Decompress::new(true);
        let decompressed = decompressor.decompress_vec(
            &buffer[44 + length..44 + length + compressed_size],
            &mut body,
            flate2::FlushDecompress::Finish,
        );
        println!("{decompressed:?}");

        let chunk_id = ChunkId::new(u32::from_le_bytes(body[0..4].try_into().unwrap()));
        println!("ChunkId: {chunk_id:#?}");
        if chunk_id.0 == 0xFACADE01 {
            println!("Invalid")
        }
        /* if chunk_id.0 & 0x11 == 0x10 {
            println!("skipable")
        } */
        /*  if chunk_id.0 & 0x11 != 0x10 {
            if chunk_id.0 & 0x10 == true {
                println!("skipable")
            }
        } */

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[44 + length + compressed_size..48 + length + compressed_size]
                .try_into()
                .unwrap(),
        ));

        println!("ChunkId: {chunk_id:#?}");
        if chunk_id.0 == 0x0303F007 {
            println!("yay")
        }

        /* let chunk_id = ClassId::new(u32::from_le_bytes(
            buffer[48 + length + compressed_size..52 + length + compressed_size]
                .try_into()
                .unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}"); */

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[52 + length + compressed_size..56 + length + compressed_size]
                .try_into()
                .unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}");
        if chunk_id.0 == 0x03092000 {
            println!("yay")
        }

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[56 + length + compressed_size..60 + length + compressed_size]
                .try_into()
                .unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}");

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[60 + length + compressed_size..64 + length + compressed_size]
                .try_into()
                .unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}");

        let chunk_id = ChunkId::new(u32::from_le_bytes(
            buffer[64 + length + compressed_size..68 + length + compressed_size]
                .try_into()
                .unwrap(),
        ));
        println!("ChunkId: {chunk_id:#?}");

        Err(GBXError::MissingMagic)
    }
}
