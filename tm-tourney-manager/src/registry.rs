use gbx_header::parse_from_buffer;
use spacetimedb::{ReducerContext, Table, reducer, table};

#[table(name = map_registry, public)]
pub struct MapRegistry {
    /// Nadeo map_uid
    #[primary_key]
    id: String,

    uploader: String,
    author: String,
    public: bool,

    /// .Map.Gbx blob
    file: Vec<u8>,
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn registry_add_map(
    ctx: &ReducerContext,
    //TODO remove
    uploader: u64,
    //map_file: Vec<u8>,
) {
    /* let map_file = include_bytes!("../../DW25 - Acchitchi.Map.Gbx");
    let map_header = parse_from_buffer(&map_file.as_slice());
    log::info!("{map_header:?}");
    let header = map_header.unwrap().header_xml;
    log::info!("{header:?}");
    ctx.db.map_registry().insert(MapRegistry {
        id: "vjyNNUu997cC5PW8e3x7Y9RsAF0".into(),
        uploader: "iyOlLqb7TMmlOwxGwIdo-g".into(),
        author: "iyOlLqb7TMmlOwxGwIdo-g".into(),
        public: true,
        file: map_file.into(),
    }); */
}
