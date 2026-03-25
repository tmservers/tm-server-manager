use std::collections::HashMap;

use spacetimedb::{DbContext, Local, ReducerContext, SpacetimeType, Table, Uuid};
use tm_server_types::config::ServerConfig;

use crate::{
    competition::{
        authorized_competition_ongoing, competition_ongoing,
        connection::{
            ConnectionRead, action::tab_connection_action, data::tab_connection_data,
            tab_connection, tab_connection__view,
        },
        tab_competition, tab_competition__view,
    },
    portal::{tab_portal, tab_portal__view},
    raw_server::player::PermittedPlayer,
    registration::{tab_registration, tab_registration__view},
    schedule::{tab_schedule, tab_schedule__view},
    tm_match::{authorized_match_set_preparation, tab_match, tab_match__view},
};
mod position;

pub use position::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy, SpacetimeType, Hash)]
#[non_exhaustive]
pub enum NodeHandle {
    MatchV1(u32),
    CompetitionV1(u32),
    MonitoringV1(u32),
    ServerV1(u32),
    ScheduleV1(u32),
    PortalV1(u32),
    RegistrationV1(u32),
}

// This is done because of a petgraph trait bound.
impl Default for NodeHandle {
    fn default() -> Self {
        log::error!(
            "Tried to call the deafault implementation of NodeKindHandle.
            This should not be possible and is only implemented because of a petgraph trait bound."
        );
        panic!()
    }
}

impl NodeHandle {
    pub(crate) fn split(self) -> (u8, u32) {
        match self {
            NodeHandle::MatchV1(m) => (1, m),
            NodeHandle::CompetitionV1(c) => (2, c),
            NodeHandle::ScheduleV1(s) => (3, s),
            NodeHandle::MonitoringV1(_) => todo!(),
            NodeHandle::ServerV1(_) => todo!(),
            NodeHandle::PortalV1(p) => (6, p),
            NodeHandle::RegistrationV1(r) => (7, r),
        }
    }

    pub(crate) fn combine(variant: u8, value: u32) -> Self {
        match variant {
            1 => Self::MatchV1(value),
            2 => Self::CompetitionV1(value),
            3 => Self::ScheduleV1(value),
            6 => Self::PortalV1(value),
            7 => Self::RegistrationV1(value),
            _ => unreachable!(),
        }
    }

    pub(crate) fn is_template(&self, ctx: &ReducerContext) -> bool {
        match self {
            NodeHandle::MatchV1(m) => {
                let node = ctx.db.tab_match().id().find(m).unwrap();
                node.is_template()
            }
            NodeHandle::CompetitionV1(c) => {
                let node = ctx.db.tab_competition().id().find(c).unwrap();
                node.is_template()
            }
            NodeHandle::ScheduleV1(s) => {
                let node = ctx.db.tab_schedule().id().find(s).unwrap();
                node.is_template()
            }
            NodeHandle::MonitoringV1(_) => todo!(),
            NodeHandle::ServerV1(_) => todo!(),
            NodeHandle::PortalV1(portal_id) => {
                let node = ctx.db.tab_portal().id().find(portal_id).unwrap();
                node.is_template()
            }
            NodeHandle::RegistrationV1(reg) => {
                let node = ctx.db.tab_registration().id().find(reg).unwrap();
                node.is_template()
            }
        }
    }

    pub(crate) fn is_match(&self) -> bool {
        match self {
            NodeHandle::MatchV1(_) => true,
            _ => false,
        }
    }
    pub(crate) fn is_server(&self) -> bool {
        match self {
            NodeHandle::ServerV1(_) => true,
            _ => false,
        }
    }
}

pub trait NodeType {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String>;
}

impl NodeType for NodeHandle {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String> {
        match self {
            NodeHandle::MatchV1(match_id) => authorized_match_set_preparation(ctx, *match_id),
            NodeHandle::CompetitionV1(c) => authorized_competition_ongoing(ctx, *c),
            NodeHandle::MonitoringV1(_) => todo!(),
            NodeHandle::ServerV1(_) => todo!(),
            NodeHandle::ScheduleV1(_) => todo!(),
            NodeHandle::PortalV1(_) => todo!(),
            NodeHandle::RegistrationV1(_) => todo!(),
        }
    }
}

pub(crate) trait NodeRead {
    fn node_permitted_players_input(&self, node: NodeHandle) -> Vec<PermittedPlayer>;
    fn node_get_parent(&self, node: NodeHandle) -> Result<u32, String>;
}
impl<Db: DbContext> NodeRead for Db {
    fn node_permitted_players_input(&self, node: NodeHandle) -> Vec<PermittedPlayer> {
        let mut map: HashMap<Uuid, PermittedPlayer> = HashMap::new();
        let depending_connections = self
            .db_read_only()
            .tab_connection()
            .origins_of()
            .filter(node.split())
            .filter(|c| c.is_data());

        for depending_connection in depending_connections {
            let permitted_players = self
                .connection_filter_permitted_players(depending_connection)
                .into_iter()
                .map(|p| (p.account_id, p));
            map.extend(permitted_players);
        }

        let values = map.into_values().collect();

        log::warn!("{:?}", values);

        values
    }

    fn node_get_parent(&self, node: NodeHandle) -> Result<u32, String> {
        match node {
            NodeHandle::MatchV1(m) => {
                if let Some(ma) = self.db_read_only().tab_match().id().find(m) {
                    Ok(ma.get_comp_id())
                } else {
                    Err("Match couldnt be found.".into())
                }
            }
            NodeHandle::CompetitionV1(c) => {
                if let Some(co) = self.db_read_only().tab_competition().id().find(c) {
                    let id = co.id;
                    if id != 0 {
                        Ok(id)
                    } else {
                        Err("Compeittion without Parent cannot be part of a connection".into())
                    }
                } else {
                    Err("Competition could not be found".into())
                }
            }
            NodeHandle::ScheduleV1(s) => {
                if let Some(ma) = self.db_read_only().tab_schedule().id().find(s) {
                    Ok(ma.parent_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
            NodeHandle::MonitoringV1(_) => todo!(),
            NodeHandle::ServerV1(_) => todo!(),
            NodeHandle::PortalV1(portal_id) => {
                if let Some(portal) = self.db_read_only().tab_portal().id().find(portal_id) {
                    Ok(portal.get_comp_id())
                } else {
                    Err("Portal could not be found.".into())
                }
            }
            NodeHandle::RegistrationV1(reg) => {
                if let Some(reg) = self.db_read_only().tab_registration().id().find(reg) {
                    Ok(reg.get_comp_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
        }
    }
}

pub(crate) trait NodeWrite: NodeRead {
    fn node_create(&self, node: NodeHandle) -> Result<(), String>;
    fn node_delete(&self, node: NodeHandle) -> Result<(), String>;
}
impl<Db: DbContext<DbView = Local>> NodeWrite for Db {
    fn node_create(&self, node: NodeHandle) -> Result<(), String> {
        self.node_position_insert(node)?;

        Ok(())
    }

    fn node_delete(&self, node: NodeHandle) -> Result<(), String> {
        // Delete postition table entry.
        self.node_position_delete(node);

        // Delete all associated connection tables involving the node.
        let connections = self
            .db()
            .tab_connection()
            .origins_of()
            .filter(node.split())
            .chain(self.db().tab_connection().targets_of().filter(node.split()));
        for connection in connections {
            if connection.is_action() {
                self.db()
                    .tab_connection_action()
                    .connection_id()
                    .delete(connection.id);
                continue;
            }
            if connection.is_data() {
                self.db()
                    .tab_connection_data()
                    .connection_id()
                    .delete(connection.id);
                continue;
            }
        }
        self.db().tab_connection().origins_of().delete(node.split());
        self.db().tab_connection().targets_of().delete(node.split());

        Ok(())
    }
}
