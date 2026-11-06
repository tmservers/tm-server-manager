use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        connection::{ConnectionStatus, tab_connection},
        node::{NodeHandle, NodeRead},
    },
    leaderboard::{
        LbEntry, LbSettingsV3,
        context::LbContext,
        error::LbError,
        evaluate_steps,
        select::{LbSelectFirstOffsetN, LbSelectPlaces, LbSelectSettings},
        shape::{LbShape, check_steps},
    },
};

/// Superseded by [`ConnectionDataV2`], only kept until `migration_connection_data_to_v2` moved the rows.
#[derive(Debug)]
#[table(accessor=tab_connection_data, vis_private)]
pub struct ConnectionData {
    #[index(hash)]
    competition_id: u32,
    #[primary_key]
    pub connection_id: u32,

    options: ConnectionDataOption,
}

#[derive(Debug, SpacetimeType)]
enum ConnectionDataOption {
    All,
    FirstN(u8),
    LastN(u8),
    Custom(Vec<u8>),
    FirstOffsetN(ConnectionDataOptionFirstOffsetN),
}

#[derive(Debug, SpacetimeType)]
struct ConnectionDataOptionFirstOffsetN {
    offset: u8,
    take: u8,
}

/// The leaderboard steps a data connection applies to the data flowing through it.
#[derive(Debug)]
#[table(accessor=tab_connection_data_v2, vis_private)]
pub struct ConnectionDataV2 {
    #[index(hash)]
    competition_id: u32,
    #[primary_key]
    pub connection_id: u32,

    settings: Vec<LbSettingsV3>,

    /// The inputs of a node are ordered by the slot of their connection, e.g. to seed the participants.
    slot: u16,
}

impl ConnectionDataV2 {
    pub(crate) fn new(connection_id: u32, competition_id: u32) -> Self {
        ConnectionDataV2 {
            competition_id,
            connection_id,
            settings: Vec::new(),
            slot: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_slot(mut self, slot: u16) -> Self {
        self.slot = slot;
        self
    }

    pub(crate) fn slot(&self) -> u16 {
        self.slot
    }

    pub(crate) fn instantiate(mut self, connection_id: u32, competition_id: u32) -> Self {
        self.competition_id = competition_id;
        self.connection_id = connection_id;
        self
    }

    pub(super) fn evaluate(
        &self,
        cx: &LbContext,
        leaderboard: Vec<LbEntry>,
    ) -> Result<Vec<LbEntry>, LbError> {
        evaluate_steps(cx, &self.settings, leaderboard)
    }

    pub(super) fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        check_steps(origin, &self.settings, input)
    }
}

impl From<ConnectionData> for ConnectionDataV2 {
    fn from(data: ConnectionData) -> Self {
        // Every option except `All` took the match standings before selecting from them.
        let places = match data.options {
            ConnectionDataOption::All => None,
            ConnectionDataOption::FirstN(n) => Some(LbSelectPlaces::FirstN(n.into())),
            ConnectionDataOption::LastN(n) => Some(LbSelectPlaces::LastN(n.into())),
            ConnectionDataOption::Custom(places) => Some(LbSelectPlaces::Custom(
                places.into_iter().map(u16::from).collect(),
            )),
            ConnectionDataOption::FirstOffsetN(opt) => {
                Some(LbSelectPlaces::FirstOffsetN(LbSelectFirstOffsetN {
                    offset: opt.offset.into(),
                    take: opt.take.into(),
                }))
            }
        };
        let settings = match places {
            Some(places) => vec![
                LbSettingsV3::Match,
                LbSettingsV3::Select(LbSelectSettings {
                    scope: Vec::new(),
                    places,
                }),
            ],
            None => Vec::new(),
        };

        ConnectionDataV2 {
            competition_id: data.competition_id,
            connection_id: data.connection_id,
            settings,
            slot: 0,
        }
    }
}

/// Moves the connection data over to the v2 table, it is safe to call this multiple times.
#[reducer]
fn migration_connection_data_to_v2(ctx: &ReducerContext) -> Result<(), String> {
    let rows: Vec<ConnectionData> = ctx.db.tab_connection_data().iter().collect();
    for row in rows {
        let connection_id = row.connection_id;
        if ctx
            .db
            .tab_connection_data_v2()
            .connection_id()
            .find(connection_id)
            .is_none()
        {
            ctx.db.tab_connection_data_v2().try_insert(row.into())?;
        }
        ctx.db
            .tab_connection_data()
            .connection_id()
            .delete(connection_id);
    }

    Ok(())
}

#[reducer]
fn competition_connection_data_update(
    ctx: &ReducerContext,
    connection_id: u32,
    settings: Vec<LbSettingsV3>,
    slot: u16,
) -> Result<(), String> {
    let Some(connection) = ctx.db.tab_connection().id().find(connection_id) else {
        return Err("connection could not be found!".into());
    };

    if connection.status != ConnectionStatus::Configuring {
        return Err("The connection can only be changed while it gets configured.".into());
    }

    ctx.auth_builder(connection.parent_id)
        .permission(CompetitionPermissionsV1::CONNECTION_CONFIGURE)
        .authorize()?;

    let origin = connection.origin();
    check_steps(origin, &settings, ctx.node_output_shape(origin)?)?;

    let Some(mut data) = ctx
        .db
        .tab_connection_data_v2()
        .connection_id()
        .find(connection_id)
    else {
        return Err("Connection data could not be found.".into());
    };

    data.settings = settings;
    data.slot = slot;

    ctx.db.tab_connection_data_v2().connection_id().update(data);

    // The target already gets what the connection passes on, even while it gets configured.
    ctx.node_check_target(connection.target())?;

    Ok(())
}

#[view(accessor = competition_connections_data,public)]
fn competition_connections_data(
    ctx: &AnonymousViewContext,
    competition_id: u32,
) -> impl Query<ConnectionDataV2> {
    ctx.from
        .tab_connection_data_v2()
        .filter(|r| r.competition_id.eq(competition_id))
}

pub(crate) trait ConnectionDataRead {
    fn connection_data_find(&self, connection_id: u32) -> Option<ConnectionDataV2>;
}
impl<Db: spacetimedb::CtxDbRead> ConnectionDataRead for Db {
    fn connection_data_find(&self, connection_id: u32) -> Option<ConnectionDataV2> {
        self.db_read_only()
            .tab_connection_data_v2()
            .connection_id()
            .find(connection_id)
    }
}

pub(crate) trait ConnectionDataWrite: ConnectionDataRead {
    fn connection_data_insert(&self, data: ConnectionDataV2) -> Result<(), String>;
    fn connection_data_delete(&self, connection_id: u32);
}
impl<Db: spacetimedb::CtxDbWrite> ConnectionDataWrite for Db {
    fn connection_data_insert(&self, data: ConnectionDataV2) -> Result<(), String> {
        self.db().tab_connection_data_v2().try_insert(data)?;
        Ok(())
    }

    fn connection_data_delete(&self, connection_id: u32) {
        self.db()
            .tab_connection_data_v2()
            .connection_id()
            .delete(connection_id);
        // A connection deleted before the migration ran would otherwise be migrated back.
        self.db()
            .tab_connection_data()
            .connection_id()
            .delete(connection_id);
    }
}
