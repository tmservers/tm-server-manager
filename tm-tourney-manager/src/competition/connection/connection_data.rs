use spacetimedb::{ReducerContext, SpacetimeType, ViewContext, reducer, table, view};

use crate::auth::Authorization;

#[derive(Debug)]
#[table(name=tab_competition_connection_data)]
pub struct CompetitionConnectionData {
    #[index(btree)]
    competition_id: u32,
    #[primary_key]
    connection_id: u32,

    count_top: Option<u8>,
    count_bottom: Option<u8>,
    custom_list: Vec<u8>,
}

impl CompetitionConnectionData {
    pub(crate) fn new(connection_id: u32, competition_id: u32) -> Self {
        CompetitionConnectionData {
            competition_id,
            connection_id,
            count_top: Some(1),
            count_bottom: None,
            custom_list: Vec::new(),
        }
    }
}

#[derive(Debug, SpacetimeType)]
pub enum CompetitionConnectionDataOption {
    First(u8),
    Last(u8),
    Custom(Vec<u8>),
}

#[view(name=competition_connection_data,public)]
pub fn competition_connection_data(ctx: &ViewContext) -> Vec<CompetitionConnectionData> {
    ctx.db
        .tab_competition_connection_data()
        .competition_id()
        //TODO actually make a view arg to filter not return everything.
        .filter(1u32..u32::MAX)
        .collect()
}

#[reducer]
fn competition_connection_data_update(
    ctx: &ReducerContext,
    option: CompetitionConnectionDataOption,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    //ctx.db.tab_competition_connection_data().
    Ok(())
}
