use spacetimedb::{Query, ReducerContext, Table, ViewContext, rand::seq::index, reducer, view};
use tm_server_types::config::ServerConfig;

use crate::{
    auth::Authorization,
    user::{tab_user__view, user_identity__view},
};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = match_template))]
pub struct MatchTemplate {
    #[auto_inc]
    #[primary_key]
    id: u32,

    #[index(btree)]
    creator: String,

    config: ServerConfig,
}

impl MatchTemplate {}

#[reducer]
fn create_match_template(ctx: &ReducerContext, config: ServerConfig) -> Result<(), String> {
    let user = ctx.get_user()? else {
        return Err("User not found".to_string());
    };

    let match_template = ctx.db.match_template().try_insert(MatchTemplate {
        id: 0,
        creator: user,
        config: config,
    });

    Ok(())
}

#[view(name=my_match_template,public)]
fn my_match_template(ctx: &ViewContext) -> Vec<MatchTemplate> {
    let id = if let Some(user) = ctx.db.user_identity().identity().find(ctx.sender) {
        user.account_id
    } else {
        return Vec::new();
    };

    ctx.db.match_template().creator().filter(&id).collect()
}
