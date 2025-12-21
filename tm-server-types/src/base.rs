mod player;
use base64::{Engine, prelude::BASE64_STANDARD};
pub use player::*;

mod team;
pub use team::Team;

mod map;
pub use map::Map;

mod time;
pub use time::*;

pub fn login_to_account_id(login: &str) -> String {
    let string = login.replace("-", "+");
    let mut string = string.replace("_", "/");

    let mut i = 0;
    while i < string.len() % 4 {
        string += "=";
        i += 1;
    }

    let bytes = BASE64_STANDARD.decode(string).unwrap();

    fn encode_hex(bytes: &[u8]) -> String {
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

pub fn account_id_to_login(account_id: &str) -> String {
    let no_dashes = account_id.replace("-", "");
    let bytes: Vec<u8> = (0..no_dashes.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&no_dashes[i..i + 2], 16).unwrap())
        .collect();
    BASE64_STANDARD.encode(bytes)
}

#[test]
fn test_account_id_to_login() {
    let account_id = String::from("3467014a-c1cc-4aae-99fe-6beb5eca232a");
    let login = account_id_to_login(&account_id);
    let new_account_id = login_to_account_id(&login);
    assert!(account_id == new_account_id);

    let account_id = String::from("8b23a52e-a6fb-4cc9-a53b-0c46c08768fa");
    let login = account_id_to_login(&account_id);
    let new_account_id = login_to_account_id(&login);
    assert!(account_id == new_account_id);

    let account_id = String::from("8c14f490-b9b5-44b3-ab01-a3c4937f3000");
    let login = account_id_to_login(&account_id);
    let new_account_id = login_to_account_id(&login);
    assert!(account_id == new_account_id);
}

#[test]
fn test_login_to_account_id() {
    let login = String::from("NGcBSsHMSq6Z_mvrXsojKg");
    let account_id = login_to_account_id(&login);
    let new_login = account_id_to_login(&account_id);
    assert!(login == new_login);

    let login = String::from("iyOlLqb7TMmlOwxGwIdo-g");
    let account_id = login_to_account_id(&login);
    let new_login = account_id_to_login(&account_id);
    assert!(login == new_login);

    let login = String::from("jBT0kLm1RLOrAaPEk38wAA");
    let account_id = login_to_account_id(&login);
    let new_login = account_id_to_login(&account_id);
    assert!(login == new_login);
}
