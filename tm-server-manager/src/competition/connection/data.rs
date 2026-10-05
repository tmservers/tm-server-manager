use spacetimedb::{ProcedureContext, ReducerContext, SpacetimeType, procedure, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        connection::{ConnectionStatus, tab_connection},
        node::NodeLeaderboard,
    },
    leaderboard::LbEntry,
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

    pub(crate) fn instantiate(mut self, connection_id: u32, competition_id: u32) -> Self {
        self.competition_id = competition_id;
        self.connection_id = connection_id;
        self
    }

    /* pub(super) fn apply_match(&self, tm_match: Vec<MatchRoundPlayer>) -> Vec<MatchRoundPlayer> {
        let players = match &self.options {
            ConnectionDataOption::All => tm_match,
            ConnectionDataOption::FirstN(f) => tm_match.into_iter().take(*f as usize).collect(),
            ConnectionDataOption::LastN(l) => {
                tm_match.into_iter().rev().take(*l as usize).collect()
            }
            ConnectionDataOption::Custom(items) => {
                let mut players = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(player) = tm_match.get((*item - 1) as usize) {
                        players.push(*player);
                    }
                }
                players
            }
            ConnectionDataOption::FirstOffsetN(opt) => tm_match
                .into_iter()
                .skip(opt.offset as usize)
                .take(opt.take as usize)
                .collect(),
        };
        players
            .into_iter()
            //.map(|p| PermittedPlayer::new(p.account_id, false, false))
            .collect()
    } */

    pub(super) fn apply_filter(
        &self,
        tm_match: Vec<LbEntry>,
        ctx: &impl spacetimedb::CtxDbRead,
    ) -> Vec<LbEntry> {
        if matches!(self.options, ConnectionDataOption::All) {
            return tm_match;
        }

        let tm_match = tm_match.finalize(ctx);

        match &self.options {
            ConnectionDataOption::All => tm_match,
            ConnectionDataOption::FirstN(f) => tm_match.into_iter().take(*f as usize).collect(),
            ConnectionDataOption::LastN(l) => {
                tm_match.into_iter().rev().take(*l as usize).collect()
            }
            ConnectionDataOption::Custom(items) => {
                let mut players = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(player) = tm_match.get((*item - 1) as usize) {
                        players.push(*player);
                    }
                }
                players
            }
            ConnectionDataOption::FirstOffsetN(opt) => tm_match
                .into_iter()
                .skip(opt.offset as usize)
                .take(opt.take as usize)
                .collect(),
        }

        /* .into_iter()
        //.map(|p| PermittedPlayer::new(p.account_id, false, false))
        .collect() */
    }

    /* pub(super) fn apply_registration(
        &self,
        registration: Vec<RegisterationPlayer>,
    ) -> Vec<RegisterationPlayer> {
        let players = match &self.options {
            ConnectionDataOption::All => registration,
            ConnectionDataOption::FirstN(f) => registration.into_iter().take(*f as usize).collect(),
            ConnectionDataOption::LastN(l) => {
                registration.into_iter().rev().take(*l as usize).collect()
            }
            ConnectionDataOption::Custom(items) => {
                let mut players = Vec::with_capacity(items.len());
                for item in items {
                    if let Some(player) = registration.get((*item - 1) as usize) {
                        players.push(*player);
                    }
                }
                players
            }
            ConnectionDataOption::FirstOffsetN(opt) => registration
                .into_iter()
                .skip(opt.offset as usize)
                .take(opt.take as usize)
                .collect(),
        };
        players
            .into_iter()
            //.map(|p| PermittedPlayer::new(p.user_id, false, false))
            .collect()
    } */
}

#[derive(Debug, SpacetimeType)]
enum ConnectionDataOption {
    All,
    FirstN(u8),
    LastN(u8),
    Custom(Vec<u8>),
    FirstOffsetN(ConnectionDataOptionFirstOffsetN),
}

/* #[view(accessor=connection_data,public)]
pub fn connection_data(
    ctx: &ViewContext, /* ,competition_id: u32 */
) -> impl Query<ConnectionData> {
    let competition_id = 1u32;
    ctx.from
        .tab_connection_data()
        .r#where(|c| c.competition_id.eq(competition_id))
} */

#[reducer]
fn competition_connection_data_update(
    ctx: &ReducerContext,
    connection_id: u32,
    option: ConnectionDataOption,
) -> Result<(), String> {
    let Some(connection) = ctx.db.tab_connection().id().find(connection_id) else {
        return Err("connection could not be found!".into());
    };

    if connection.status == ConnectionStatus::Resolved {
        return Err("Wrong status to chnge config".into());
    }

    ctx.auth_builder(connection.parent_id)
        .permission(CompetitionPermissionsV1::COMPETITION_CONNECTION_EDIT)
        .authorize()?;

    let Some(mut data) = ctx
        .db
        .tab_connection_data()
        .connection_id()
        .find(connection_id)
    else {
        return Err("Connection data could not be found.".into());
    };

    data.options = option;

    ctx.db.tab_connection_data().connection_id().update(data);

    Ok(())
}

#[derive(Debug, SpacetimeType)]
struct ConnectionDataOptionFirstOffsetN {
    offset: u8,
    take: u8,
}

#[procedure(name = "unstable_competition_connection_data")]
fn unstable_competition_connection_data(
    ctx: &mut ProcedureContext,
    competition_id: u32,
) -> Vec<ConnectionData> {
    ctx.with_tx(|ctx| {
        ctx.db
            .tab_connection_data()
            .competition_id()
            .filter(competition_id)
    })
    .collect()
}
