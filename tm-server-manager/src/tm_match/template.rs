use spacetimedb::{Query, ReducerContext, Table, ViewContext, reducer, view};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, tab_competition},
    tm_match::{MatchStatus, MatchV1, tab_match, tab_match__query},
};

#[reducer]
fn match_template_create(ctx: &ReducerContext, name: String, parent_id: u32) -> Result<(), String> {
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::MATCH_CREATE)
        .authorize()?;

    ctx.db.tab_match().try_insert(MatchV1 {
        id: 0,
        parent_id,
        name,
        status: MatchStatus::Configuring,
        pre_config: 0,
        config: 0,
        auto_provision_server: true,
        template: true,
        open: false,
    })?;

    Ok(())
}

pub(super) fn match_template_instantiate(
    ctx: &ReducerContext,
    match_id: u32,
) -> Result<(), String> {
    todo!()
}

#[view(accessor=my_match_template,public)]
fn my_match_template(ctx: &ViewContext /* , competition_id: u32 */) -> impl Query<MatchV1> {
    let competition_id = 1u32;
    // TODO: return only users own templates
    ctx.from.tab_match().build()
}
