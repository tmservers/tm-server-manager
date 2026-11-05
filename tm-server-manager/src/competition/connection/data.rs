use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        connection::{ConnectionRead, ConnectionStatus, tab_connection},
        node::{NodeHandle, NodeRead},
    },
    pipeline::{
        LbEntry, LbSettingsV3,
        context::LbContext,
        error::LbError,
        evaluate_steps,
        shape::{LbShape, check_steps},
        steps::{
            script::script_permission,
            select::{LbSelectFirstOffsetN, LbSelectPlaces, LbSelectSettings},
        },
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

    /// The inputs of a node are ordered by the slot of their connection, e.g. to seed the participants.
    /// Every input of a node has its own slot, only 0 can be shared by inputs whose order does not matter.
    slot: u16,

    settings: Vec<LbSettingsV3>,
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

impl TryFrom<ConnectionData> for ConnectionDataV2 {
    type Error = String;

    fn try_from(data: ConnectionData) -> Result<Self, Self::Error> {
        // Every option except `All` took the match standings before selecting from them.
        let places = match data.options {
            ConnectionDataOption::All => None,
            ConnectionDataOption::FirstN(n) => Some(LbSelectPlaces::FirstN(n.into())),
            // Nothing used it, so selecting the last places got removed.
            ConnectionDataOption::LastN(_) => {
                return Err(format!(
                    "Connection {} selects the last places, which cannot be migrated.",
                    data.connection_id
                ));
            }
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

        Ok(ConnectionDataV2 {
            competition_id: data.competition_id,
            connection_id: data.connection_id,
            settings,
            slot: 0,
        })
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
            ctx.db
                .tab_connection_data_v2()
                .try_insert(row.try_into()?)?;
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

    let Some(mut data) = ctx
        .db
        .tab_connection_data_v2()
        .connection_id()
        .find(connection_id)
    else {
        return Err("Connection data could not be found.".into());
    };

    ctx.auth_builder(connection.parent_id)
        .permission(CompetitionPermissionsV1::CONNECTION_CONFIGURE)
        .permission(script_permission(&data.settings, &settings))
        .authorize()?;

    let origin = connection.origin();
    check_steps(origin, &settings, ctx.node_output_shape(origin)?)?;

    if let Some(taken) = slot_taken_by(ctx, connection.target(), slot, connection_id) {
        return Err(format!(
            "Slot {slot} is already used by connection {taken} into {:?}.",
            connection.target()
        ));
    }

    data.settings = settings;
    data.slot = slot;

    ctx.db.tab_connection_data_v2().connection_id().update(data);

    // The target already gets what the connection passes on, even while it gets configured.
    ctx.node_check_target(connection.target())?;

    Ok(())
}

/// The other data connection into `target` which already has the slot, slot 0 can be shared.
fn slot_taken_by(
    ctx: &impl spacetimedb::CtxDbRead,
    target: NodeHandle,
    slot: u16,
    connection_id: u32,
) -> Option<u32> {
    if slot == 0 {
        return None;
    }
    ctx.connection_origins_of(target)
        .filter(|c| c.is_data() && c.id != connection_id)
        .find(|c| {
            ctx.connection_data_find(c.id)
                .is_some_and(|data| data.slot == slot)
        })
        .map(|c| c.id)
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

#[cfg(test)]
mod tests {
    use spacetimedb::test_utils::{TestAuth, TestContext};
    use tm_server_types::config::TmMode;

    use super::*;
    use crate::{
        competition::connection::{ConnectionKind, ConnectionWrite, TabConnection},
        pipeline::test_util::test_context,
        tm_match::leaderboard::{MatchLeaderboardWrite, MatchRoundPlayer},
    };

    /// Connects every origin to the target with its slot, returns the connection ids.
    fn connect(test: &TestContext, target: NodeHandle, inputs: &[(NodeHandle, u16)]) -> Vec<u32> {
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            let mut ids = Vec::new();
            for &(origin, slot) in inputs {
                let connection = ctx.connection_insert(TabConnection::for_test(
                    origin,
                    target,
                    ConnectionKind::Data,
                ))?;
                ctx.connection_data_insert(
                    ConnectionDataV2::new(connection.id, 1).with_slot(slot),
                )?;
                ids.push(connection.id);
            }
            Ok::<_, String>(ids)
        })
        .unwrap()
    }

    #[test]
    fn a_slot_can_only_be_used_once_per_target_but_0_can_be_shared() {
        let test = test_context(&[]);
        let (target, other) = (NodeHandle::MatchV1(9), NodeHandle::MatchV1(8));
        let ids = connect(
            &test,
            target,
            &[(NodeHandle::MatchV1(1), 1), (NodeHandle::MatchV1(2), 0)],
        );
        connect(&test, other, &[(NodeHandle::MatchV1(3), 2)]);
        let cx = test.anonymous_view_context();
        let new_connection = 100;

        assert_eq!(slot_taken_by(&cx, target, 1, new_connection), Some(ids[0]));
        // A connection keeping its own slot does not collide with itself.
        assert_eq!(slot_taken_by(&cx, target, 1, ids[0]), None);
        assert_eq!(slot_taken_by(&cx, target, 0, new_connection), None);
        // Slots of other targets do not count.
        assert_eq!(slot_taken_by(&cx, target, 2, new_connection), None);
    }

    #[test]
    fn inputs_come_in_slot_order_and_those_without_a_slot_last() {
        let test = test_context(&[
            (1, TmMode::Rounds),
            (2, TmMode::Rounds),
            (3, TmMode::Rounds),
        ]);
        let target = NodeHandle::MatchV1(9);
        test.with_reducer_tx(TestAuth::internal(), |ctx| {
            // Every match is played by a single user, so the users show the order of the inputs.
            for match_id in 1..=3 {
                ctx.match_round_player_insert(MatchRoundPlayer::for_test(
                    match_id,
                    match_id * 10,
                    1,
                    0,
                ))?;
            }
            Ok::<_, String>(())
        })
        .unwrap();
        connect(
            &test,
            target,
            &[
                (NodeHandle::MatchV1(1), 0),
                (NodeHandle::MatchV1(2), 2),
                (NodeHandle::MatchV1(3), 1),
            ],
        );

        let inputs = test
            .anonymous_view_context()
            .node_resolve_inputs(target)
            .unwrap();

        let users: Vec<u32> = inputs.iter().map(|input| input[0].user_id).collect();
        assert_eq!(users, vec![30, 20, 10]);
    }

    fn migrated(options: ConnectionDataOption) -> Result<Vec<LbSettingsV3>, String> {
        ConnectionDataV2::try_from(ConnectionData {
            competition_id: 1,
            connection_id: 2,
            options,
        })
        .map(|data| data.settings)
    }

    #[test]
    fn migrated_options_select_from_the_match_standings() {
        assert!(migrated(ConnectionDataOption::All).unwrap().is_empty());
        assert!(matches!(
            migrated(ConnectionDataOption::FirstN(3))
                .unwrap()
                .as_slice(),
            [
                LbSettingsV3::Match,
                LbSettingsV3::Select(LbSelectSettings {
                    places: LbSelectPlaces::FirstN(3),
                    ..
                })
            ]
        ));
        // Selecting the last places does not exist anymore.
        assert!(migrated(ConnectionDataOption::LastN(3)).is_err());
    }
}
