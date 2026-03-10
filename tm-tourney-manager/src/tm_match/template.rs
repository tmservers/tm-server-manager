use spacetimedb::{Query, ReducerContext, Table, Uuid, ViewContext, reducer, table, view};
use tm_server_types::config::ServerConfig;

use crate::{
    authorization::Authorization,
    competition::tab_competition,
    project::permissions::ProjectPermissionsV1,
    raw_server::config::{RawServerConfig, tab_raw_server_config},
    tm_match::{MatchStatus, TmMatchV1, tab_tm_match, tab_tm_match__query},
};

/* #[table(accessor= tab_match_template)]
pub struct MatchTemplate {
    name: String,

    #[index(btree)]
    creator: Uuid,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    config_id: u32,
}

impl MatchTemplate {
    pub(crate) fn get_config_id(&self) -> u32 {
        self.config_id
    }
} */

#[reducer]
fn match_template_create(ctx: &ReducerContext, name: String, parent_id: u32) -> Result<(), String> {
    let user = ctx.get_user_account()?;

    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(parent_competition.get_project(), user)?
        .permission(ProjectPermissionsV1::MATCH_CREATE)
        .authorize()?;

    ctx.db.tab_tm_match().try_insert(TmMatchV1 {
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
    ctx.from.tab_tm_match().build()
}
