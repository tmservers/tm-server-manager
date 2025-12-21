mod player;
use base64::{Engine, prelude::BASE64_STANDARD};
pub use player::*;

mod team;
pub use team::Team;

mod map;
pub use map::Map;

mod time;
pub use time::*;

pub fn convert_login_into_ubiid(string: String) -> String {
    let string = string.replace("-", "+");
    let mut string = string.replace("_", "/");

    let mut i = 0;
    while i < string.len() % 4 {
        string += "=";
        i += 1;
    }

    let bytes = BASE64_STANDARD.decode(string).unwrap();

    pub fn encode_hex(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            write!(&mut s, "{:02x}", b).unwrap();
        }
        s
    }

    let i_dont_want_to_anymore = encode_hex(&bytes[0..4])
        + "-"
        + &encode_hex(&bytes[4..6])
        + "-"
        + &encode_hex(&bytes[6..8])
        + "-"
        + &encode_hex(&bytes[8..10])
        + "-"
        + &encode_hex(&bytes[10..16]);

    i_dont_want_to_anymore.to_lowercase()
}
