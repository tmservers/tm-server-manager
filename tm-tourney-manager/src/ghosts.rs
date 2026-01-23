use spacetimedb::{ProcedureContext, ProcedureResult, ReducerContext, http::Request, procedure};

use crate::{authorization::Authorization, environment::env};

#[cfg_attr(feature="spacetime",spacetimedb::table(name = match_ghost,public))]
pub struct MatchGhost {
    //id doesnt tell me anything
    //#[cfg_attr(feature = "spacetime", auto_inc)]
    //id: u32,

    //TODO
    tournament_id: u32,
    match_id: u32,
    //map_uid
    player_id: String,
    uid: String,
}

#[procedure]
pub fn post_round_replay(ctx: &mut ProcedureContext, replay: Vec<u8>) {
    //TODO: This unwrap is bad but i cant return a Result<T,E> and ? the call atm because its no spacetme type.
    let server = ctx.try_with_tx(|ctx| ctx.get_server()).unwrap();

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

    let request = Request::builder()
        .method("PUT")
        .uri("https://s3.eu-central-003.backblazeb2.com/tm-tourney-manager-staging/whatever")
        .body(replay)
        .unwrap();
    let result = ctx.http.send(request).unwrap();

    /* ctx.db.match_ghost().insert(MatchGhost {
        uid: "TODO".into(),
        tournament_id: 0,
        match_id: 0,
        player_id: "TODO".into(),
    }); */

    //Ok(())
}
