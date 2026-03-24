use spacetimedb::{Query, ReducerContext, Table, ViewContext, reducer, view};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, tab_competition},
    tm_match::{MatchStatus, TmMatchV1, tab_match, tab_match__query},
    tm_server::{TmServerV1, tab_server},
};

#[reducer]
fn server_template_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
) -> Result<(), String> {
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CREATE)
        .authorize()?;

    ctx.db.tab_server().try_insert(TmServerV1 {
        name,
        id: 0,
        parent_id,
        config: 0,
        status: crate::tm_server::ServerStatus::Configuring,
        open: true,
    })?;

    Ok(())
}

pub(super) fn server_template_instantiate(
    ctx: &ReducerContext,
    match_id: u32,
) -> Result<(), String> {
    todo!()
}
