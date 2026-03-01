use spacetimedb::{ProcedureContext, procedure};

use crate::{environment::env, raw_server::tab_raw_server};

#[procedure]
pub fn post_round_replay(ctx: &mut ProcedureContext, replay: Vec<u8>) -> Result<(), String> {
    let sender = ctx.sender();
    let server = ctx.try_with_tx(|ctx| {
        let Some(user_account) = ctx.db.tab_raw_server().identity().find(sender) else {
            return Err("Identity not associated with a user account.".to_string());
        };

        Ok(user_account)
    })?;

    //server.active_match()

    //TODO: Parse the replay file into individual ghosts.
    //let files = gamebox_files::try_parse_buffer(&replay);

    //TODO get all the information from the server -> match and stuff.

    let base_url = ctx
        .try_with_tx(
            |ctx| match ctx.db.env().key().find("S3_BASE_URL".to_string()) {
                Some(var) => Ok(var.value),
                None => Err("Key of environment variable not found.".to_string()),
            },
        )
        .unwrap();

    let key_id = ctx
        .try_with_tx(
            |ctx| match ctx.db.env().key().find("S3_KEY_ID".to_string()) {
                Some(var) => Ok(var.value),
                None => Err("Key of environment variable not found.".to_string()),
            },
        )
        .unwrap();

    let key_secret = ctx
        .try_with_tx(
            |ctx| match ctx.db.env().key().find("S3_KEY_SECRET".to_string()) {
                Some(var) => Ok(var.value),
                None => Err("Key of environment variable not found.".to_string()),
            },
        )
        .unwrap();

    /* let request = Request::builder()
        .method("PUT")
        .uri("https://s3.eu-central-003.backblazeb2.com/tm-tourney-manager-staging/whatever")
        .body(replay)
        .unwrap();
    let result = ctx.http.send(request).unwrap(); */

    /* ctx.db.match_ghost().insert(MatchGhost {
        uid: "TODO".into(),
        project_id: 0,
        match_id: 0,
        player_id: "TODO".into(),
    }); */

    Ok(())
}
