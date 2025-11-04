use spacetimedb::ReducerContext;

#[cfg_attr(feature="spacetime",spacetimedb::table(name = ghost,public))]
pub struct Ghost {
    #[cfg_attr(feature = "spacetime", auto_inc)]
    id: u64,

    //TODO
    //tournament
    //match
    //map_uid
    //player_uid
    //link to CF R2 most likely (object id)
    data: Vec<u8>,
}

#[cfg(feature = "spacetime")]
#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
pub fn post_ghost(ctx: &ReducerContext, ghost: Vec<u8>) {
    use spacetimedb::Table;

    ctx.db.ghost().insert(Ghost { id: 0, data: ghost });
}
