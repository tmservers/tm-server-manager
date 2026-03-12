use spacetimedb::{ReducerContext, SpacetimeType, ViewContext, reducer, table, view};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, connection::tab_competition_connection},
    raw_server::player::PermittedPlayer,
    tm_match::leaderboard::MatchRoundPlayer,
};

#[derive(Debug)]
#[table(accessor=tab_competition_connection_data)]
pub struct CompetitionConnectionData {
    #[index(btree)]
    competition_id: u32,
    #[primary_key]
    pub connection_id: u32,

    options: CompetitionConnectionDataOption,
}

impl CompetitionConnectionData {
    pub(crate) fn new(connection_id: u32, competition_id: u32) -> Self {
        CompetitionConnectionData {
            competition_id,
            connection_id,
            options: CompetitionConnectionDataOption::All,
        }
    }

    pub(super) fn apply_match(&self, tm_match: Vec<MatchRoundPlayer>) -> Vec<PermittedPlayer> {
        let players = match &self.options {
            CompetitionConnectionDataOption::None => return Vec::new(),
            CompetitionConnectionDataOption::All => tm_match,
            CompetitionConnectionDataOption::First(f) => {
                tm_match.into_iter().take(*f as usize).collect()
            }
            CompetitionConnectionDataOption::Last(l) => {
                tm_match.into_iter().rev().take(*l as usize).collect()
            }
            CompetitionConnectionDataOption::Custom(items) => {
                let mut players = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(player) = tm_match.get(*item as usize) {
                        players.push(*player);
                    }
                }
                players
            }
        };
        players
            .into_iter()
            .map(|p| PermittedPlayer::new(p.account_id, false, false))
            .collect()
    }
}

#[derive(Debug, SpacetimeType)]
pub enum CompetitionConnectionDataOption {
    //TODO evaluate this appraoch. probably its bad
    None,
    All,
    First(u8),
    Last(u8),
    Custom(Vec<u8>),
}

#[view(accessor=competition_connection_data,public)]
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
    connection_id: u32,
    option: CompetitionConnectionDataOption,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;

    let Some(connection) = ctx.db.tab_competition_connection().id().find(connection_id) else {
        return Err("connection could not be found!".into());
    };
    ctx.auth_builder(connection.parent_id, account_id)?
        .permission(CompetitionPermissionsV1::COMPETITION_CONNECTION_EDIT)
        .authorize()?;

    let Some(mut data) = ctx
        .db
        .tab_competition_connection_data()
        .connection_id()
        .find(connection_id)
    else {
        return Err("Connection could not be found.".into());
    };

    data.options = option;

    ctx.db
        .tab_competition_connection_data()
        .connection_id()
        .update(data);

    Ok(())
}
