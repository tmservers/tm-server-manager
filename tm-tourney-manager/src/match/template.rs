use spacetimedb::{rand::seq::index, reducer, view, Query, ReducerContext, Table, ViewContext};
use tm_server_types::config::ServerConfig;

use crate::{
    auth::Authorization,
    user::{tab_user__view, user_identity__view},
};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = match_template))]
pub struct MatchTemplate {
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(btree)]
    creator: String,

    name: String,

    pub config: ServerConfig,
}

impl MatchTemplate {}

#[reducer]
fn create_match_template(
    ctx: &ReducerContext,
    name: String,
    config: ServerConfig,
) -> Result<(), String> {
    let user = ctx.get_user()? else {
        return Err("User not found".to_string());
    };

    let match_template = ctx.db.match_template().try_insert(MatchTemplate {
        id: 0,
        creator: user,
        name: name,
        config: config,
    });

    Ok(())
}

#[view(name=my_match_template,public)]
fn my_match_template(ctx: &ViewContext) -> Query<MatchTemplate> {
    // TODO: return only users own templates
    ctx.from.match_template().build()
}
