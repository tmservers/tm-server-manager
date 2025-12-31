use spacetimedb::{ReducerContext, rand::seq::index, reducer, Table};
use tm_server_types::config::ServerConfig;

use crate::{
	auth::Authorization,
	user::{tab_user__view, user_identity__view},
};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = match_template,public))]
pub struct MatchTemplate {
    #[auto_inc]
    #[primary_key]
    id: u32,

	#[index(btree)]
    creator: String,

	config: Option<ServerConfig>,
}

impl MatchTemplate {}

#[reducer]
fn create_match_template(
	ctx: &ReducerContext,
	config: Option<ServerConfig>
) -> Result<(), String> {
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
