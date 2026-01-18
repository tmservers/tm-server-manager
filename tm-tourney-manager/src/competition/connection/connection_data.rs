use spacetimedb::{ViewContext, table, view};

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
            count_top: None,
            count_bottom: None,
            connection_id,
            custom_list: Vec::new(),
        }
    }
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
