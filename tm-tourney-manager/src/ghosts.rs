use spacetimedb::{ProcedureContext, ProcedureResult, ReducerContext, procedure};

use crate::auth::Authorization;

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

#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn post_ghost(ctx: &ReducerContext, ghost: Vec<u8>) -> Result<(), String> {
    use spacetimedb::Table;

    use crate::auth::Authorization;

    let server = ctx.auth_server()?;
    //TODO get all the information from the server -> match and stuff.

    //TODO make http call to save in object storage.
    // AHHH HOW TO ENSURE THAT the next round doesnt start before all ghosts are uploaded?
    //maybe dump the replay file and parse server side.
    // or a second arg with the round must be provided to the reducer.

    ctx.db.match_ghost().insert(MatchGhost {
        uid: "TODO".into(),
        tournament_id: 0,
        match_id: 0,
        player_id: "TODO".into(),
    });

    Ok(())
}

#[procedure]
pub fn post_replay(ctx: &mut ProcedureContext, replay: Vec<u8>) {
    //let files = gamebox_files::try_parse_buffer(&replay);

    //TODO: This unwrap is bad but i cant return a Result<T,E> and ? the call atm because its no spacetme type.
    let server = ctx.try_with_tx(|ctx| ctx.auth_server()).unwrap();

    //let server = ctx.auth_server()?;
    //TODO get all the information from the server -> match and stuff.

    //TODO make http call to save in object storage.
    // AHHH HOW TO ENSURE THAT the next round doesnt start before all ghosts are uploaded?
    //maybe dump the replay file and parse server side.
    // or a second arg with the round must be provided to the reducer.

    /* ctx.db.match_ghost().insert(MatchGhost {
        uid: "TODO".into(),
        tournament_id: 0,
        match_id: 0,
        player_id: "TODO".into(),
    }); */

    //Ok(())
}
