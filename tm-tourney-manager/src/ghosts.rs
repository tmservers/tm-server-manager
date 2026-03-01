use spacetimedb::{
    ProcedureContext, ProcedureResult, ReducerContext, Uuid, http::Request, procedure, table,
};

use crate::{authorization::Authorization, environment::env};

#[table(accessor= tab_match_ghost)]
pub struct MatchGhost {
    //id doesnt tell me anything
    //#[cfg_attr(feature = "spacetime", auto_inc)]
    //id: u32,

    //TODO
    project_id: u32,
    match_id: u32,
    //map_uid
    player_id: Uuid,
    uid: Uuid,
}
