use spacetimedb::ReducerContext;

#[cfg_attr(feature="spacetime",spacetimedb::table(name = match_ghost,public))]
pub struct MatchGhost {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    id: u64,

    //TODO
    //tournament
    //match
    //map_uid
    //player_uid
    link: String,
}

#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn post_ghost(ctx: &ReducerContext, ghost: Vec<u8>) -> Result<(), String> {
    use spacetimedb::Table;

    use crate::auth::Authorization;

    ctx.auth_user()?;

    //TODO make http call to save in object storage.
    // AHHH HOW TO ENSURE THAT the next round doesnt start before all ghosts are uploaded?
    //maybe dump the replay file and parse server side.

    ctx.db.match_ghost().insert(MatchGhost {
        id: 0,
        link: "TODO".into(),
    });

    Ok(())
}
