use spacetimedb::{Query, ReducerContext, Table, Uuid, ViewContext, reducer, table, view};
use tm_server_types::config::ServerConfig;

use crate::{
    authorization::Authorization,
    raw_server::config::{RawServerConfig, tab_raw_server_config},
};

#[table(name = tab_match_template)]
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
}

#[reducer]
fn match_template_create(
    ctx: &ReducerContext,
    name: String,
    config: ServerConfig,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let config = ctx
        .db
        .tab_raw_server_config()
        .try_insert(RawServerConfig::new(config))?;

    ctx.db.tab_match_template().try_insert(MatchTemplate {
        id: 0,
        creator: user.account_id,
        name,
        config_id: config.id,
    })?;

    Ok(())
}

#[view(name=my_match_template,public)]
fn my_match_template(ctx: &ViewContext) -> Query<MatchTemplate> {
    // TODO: return only users own templates
    ctx.from.tab_match_template().build()
}
