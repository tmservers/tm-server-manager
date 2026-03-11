use spacetimedb::{Query, ReducerContext, Table, ViewContext, reducer, view};

use crate::{
    authorization::Authorization,
    competition::tab_competition,
    project::permissions::ProjectPermissionsV1,
    tm_match::{MatchStatus, TmMatchV1, tab_match, tab_match__query},
};

#[reducer]
fn match_template_create(ctx: &ReducerContext, name: String, parent_id: u32) -> Result<(), String> {
    let user = ctx.get_user_account()?;

    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(parent_competition.get_project(), user)?
        .permission(ProjectPermissionsV1::MATCH_CREATE)
        .authorize()?;

    ctx.db.tab_match().try_insert(TmMatchV1 {
        id: 0,
        parent_id,
        project_id: parent_competition.get_project(),
        name,
        status: MatchStatus::Configuring,
        pre_match_config: 0,
        match_config: 0,
        post_match_config: 0,
        auto_provision_server: true,
        template: true,
        restricted: true,
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
fn my_match_template(ctx: &ViewContext /* , competition_id: u32 */) -> impl Query<TmMatchV1> {
    let competition_id = 1u32;
    // TODO: return only users own templates
    ctx.from.tab_match().build()
}
