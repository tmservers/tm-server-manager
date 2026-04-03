use spacetimedb::{ProcedureContext, Table, Uuid, http::Request, procedure, table};

use crate::{
    env::env,
    raw_server::{occupation::TabRawServerOccupationRead, tab_raw_server},
    tm_match::replay::MatchReplayWrite,
};

#[procedure]
pub fn post_round_replay(
    ctx: &mut ProcedureContext,
    count: u16,
    replay: Vec<u8>,
) -> Result<(), String> {
    let sender = ctx.sender();

    ctx.try_with_tx(|ctx| {
        let server = match ctx.db.tab_raw_server().identity().find(sender) {
            Some(server) => server,
            None => return Err("Identity not associated with a server account.".to_string()),
        };

        let Some(occupation) = ctx.raw_server_occupation(server.id) else {
            return Err("Server is not occupied.".into());
        };

        if occupation.is_match() {
            //occupation.id()
            //ctx.insert_match_round_replay(occupation.id(), round, map_id, replay);
        }

        Ok(())
    })?;

    Ok(())
}

/* let (server, base_url, key_id, key_secret) = ctx.try_with_tx(|ctx| {
    Ok((
        match ctx.db.tab_raw_server().identity().find(sender) {
            Some(server) => server,
            None => return Err("Identity not associated with a server account.".to_string()),
        },
        match ctx.db.env().key().find("S3_BASE_URL".to_string()) {
            Some(var) => var.value,
            None => return Err("Key of environment variable not found.".to_string()),
        },
        match ctx.db.env().key().find("S3_KEY_ID".to_string()) {
            Some(var) => var.value,
            None => return Err("Key of environment variable not found.".to_string()),
        },
        match ctx.db.env().key().find("S3_KEY_SECRET".to_string()) {
            Some(var) => var.value,
            None => return Err("Key of environment variable not found.".to_string()),
        },
    ))
})?;

let object_id = ctx.new_uuid_v7().map_err(|e| e.to_string())?;

//TODO idk how to authenticate with this.
let request = Request::builder()
    .method("PUT")
    .uri(format!(
        "https://s3.eu-central-003.backblazeb2.com/tmservers/{}",
        object_id
    ))
    .body(replay)
    .map_err(|e| e.to_string())?;
let result = ctx.http.send(request).map_err(|e| e.to_string())?;

if result.status().is_success() {
    ctx.try_with_tx::<(), String>(|ctx| {
        ctx.db
            .tab_match_round_replay()
            .try_insert(MatchRoundReplay::new(todo!(), todo!(), todo!(), object_id))?;
        Ok(())
    })?;
} else {
    log::error!("Replay could no be posted.")
} */
