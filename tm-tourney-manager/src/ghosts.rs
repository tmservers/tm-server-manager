use spacetimedb::ReducerContext;

#[cfg_attr(feature="spacetime",spacetimedb::table(name = match_ghost,public))]
pub struct MatchGhost {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    id: u32,

    //TODO
    //tournament
    //match
    //map_uid
    //player_uid
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
        id: 0,
        uid: "TODO".into(),
    });

    Ok(())
}
