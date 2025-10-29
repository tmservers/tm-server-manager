use gamebox_files::try_parse_buffer;

fn main() {
    let file = std::fs::read(env!("CARGO_MANIFEST_DIR").to_string() + "./test_replay.Gbx").unwrap();
    println!("{:?}", try_parse_buffer(file));
}
