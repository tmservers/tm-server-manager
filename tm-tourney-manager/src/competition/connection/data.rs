use spacetimedb::{Query, ReducerContext, SpacetimeType, ViewContext, reducer, table, view};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        connection::{CompetitionConnection, tab_connection},
    },
    raw_server::player::PermittedPlayer,
    tm_match::leaderboard::MatchRoundPlayer,
};

#[derive(Debug)]
#[table(accessor=tab_connection_data)]
pub struct ConnectionData {
    #[index(hash)]
    competition_id: u32,
    #[primary_key]
    pub connection_id: u32,

    options: ConnectionDataOption,
}

impl ConnectionData {
    pub(crate) fn new(connection_id: u32, competition_id: u32) -> Self {
        ConnectionData {
            competition_id,
            connection_id,
            options: ConnectionDataOption::All,
        }
    }

    pub(super) fn apply_match(&self, tm_match: Vec<MatchRoundPlayer>) -> Vec<PermittedPlayer> {
        let players = match &self.options {
            ConnectionDataOption::None => return Vec::new(),
            ConnectionDataOption::All => tm_match,
            ConnectionDataOption::First(f) => tm_match.into_iter().take(*f as usize).collect(),
            ConnectionDataOption::Last(l) => tm_match.into_iter().rev().take(*l as usize).collect(),
            ConnectionDataOption::Custom(items) => {
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
pub enum ConnectionDataOption {
    //TODO evaluate this appraoch. probably its bad
    None,
    All,
    First(u8),
    Last(u8),
    Custom(Vec<u8>),
}

#[view(accessor=competition_connection_data,public)]
pub fn competition_connection_data(
    ctx: &ViewContext, /* ,competition_id: u32 */
) -> impl Query<ConnectionData> {
    let competition_id = 1u32;
    ctx.from
        .tab_connection_data()
        .r#where(|c| c.competition_id.eq(competition_id))
}

#[reducer]
fn competition_connection_data_update(
    ctx: &ReducerContext,
    connection_id: u32,
    option: ConnectionDataOption,
) -> Result<(), String> {
    let Some(connection) = ctx.db.tab_connection().id().find(connection_id) else {
        return Err("connection could not be found!".into());
    };
    ctx.auth_builder(connection.parent_id)
        .permission(CompetitionPermissionsV1::COMPETITION_CONNECTION_EDIT)
        .authorize()?;

    let Some(mut data) = ctx
        .db
        .tab_connection_data()
        .connection_id()
        .find(connection_id)
    else {
        return Err("Connection could not be found.".into());
    };

    data.options = option;

    ctx.db.tab_connection_data().connection_id().update(data);

    Ok(())
}
