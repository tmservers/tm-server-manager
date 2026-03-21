use spacetimedb::{ReducerContext, SpacetimeType};

use crate::{
    competition::tab_competition,
    portal::tab_portal,
    registration::tab_registration,
    schedule::tab_schedule,
    tm_match::{match_set_preparation, tab_match},
};

mod position;

pub use position::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy, SpacetimeType, Hash)]
#[non_exhaustive]
pub enum NodeKindHandle {
    MatchV1(u32),
    CompetitionV1(u32),
    MonitoringV1(u32),
    ServerV1(u32),
    ScheduleV1(u32),
    PortalV1(u32),
    RegistrationV1(u32),
}

// This is done because of a petgraph trait bound.
impl Default for NodeKindHandle {
    fn default() -> Self {
        log::error!(
            "Tried to call the deafault implementation of NodeKindHandle.
            This should not be possible and is only implemented because of a petgraph trait bound."
        );
        panic!()
    }
}

impl NodeKindHandle {
    pub(crate) fn get_competition(&self, ctx: &ReducerContext) -> Result<u32, String> {
        match self {
            NodeKindHandle::MatchV1(m) => {
                if let Some(ma) = ctx.db.tab_match().id().find(m) {
                    Ok(ma.get_comp_id())
                } else {
                    Err("Match couldnt be found.".into())
                }
            }
            NodeKindHandle::CompetitionV1(c) => {
                if let Some(co) = ctx.db.tab_competition().id().find(c) {
                    let id = co.get_comp_id();
                    if id != 0 {
                        Ok(id)
                    } else {
                        Err("Compeittion without Parent cannot be part of a connection".into())
                    }
                } else {
                    Err("Competition could not be found".into())
                }
            }
            NodeKindHandle::ScheduleV1(s) => {
                if let Some(ma) = ctx.db.tab_schedule().id().find(s) {
                    Ok(ma.parent_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(portal_id) => {
                if let Some(portal) = ctx.db.tab_portal().id().find(*portal_id) {
                    Ok(portal.get_comp_id())
                } else {
                    Err("Portal could not be found.".into())
                }
            }
            NodeKindHandle::RegistrationV1(reg) => {
                if let Some(reg) = ctx.db.tab_registration().id().find(reg) {
                    Ok(reg.get_comp_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
        }
    }

    /* /// Safety: can only be called when you know the competiiton exists
    pub(crate) fn get_project(&self, ctx: &ReducerContext) -> u32 {
        match self {
            NodeKindHandle::MatchV1(m) => {
                if let Some(ma) = ctx.db.tab_match().id().find(m) {
                    ma.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::CompetitionV1(c) => {
                if let Some(co) = ctx.db.tab_competition().id().find(c) {
                    co.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::ScheduleV1(s) => {
                if let Some(ma) = ctx.db.tab_schedule().id().find(s) {
                    ma.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(port) => {
                if let Some(portal) = ctx.db.tab_portal().id().find(port) {
                    portal.get_project()
                } else {
                    u32::MAX
                }
            }
            NodeKindHandle::RegistrationV1(reg) => {
                if let Some(reg) = ctx.db.tab_registration().id().find(reg) {
                    reg.get_project()
                } else {
                    u32::MAX
                }
            }
        }
    } */

    pub(crate) fn split(self) -> (u8, u32) {
        match self {
            NodeKindHandle::MatchV1(m) => (1, m),
            NodeKindHandle::CompetitionV1(c) => (2, c),
            NodeKindHandle::ScheduleV1(s) => (3, s),
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(p) => (6, p),
            NodeKindHandle::RegistrationV1(r) => (7, r),
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
            NodeKindHandle::MatchV1(m) => {
                let node = ctx.db.tab_match().id().find(m).unwrap();
                node.is_template()
            }
            NodeKindHandle::CompetitionV1(c) => {
                let node = ctx.db.tab_competition().id().find(c).unwrap();
                node.is_template()
            }
            NodeKindHandle::ScheduleV1(s) => {
                let node = ctx.db.tab_schedule().id().find(s).unwrap();
                node.is_template()
            }
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::PortalV1(portal_id) => {
                let node = ctx.db.tab_portal().id().find(portal_id).unwrap();
                node.is_template()
            }
            NodeKindHandle::RegistrationV1(reg) => {
                let node = ctx.db.tab_registration().id().find(reg).unwrap();
                node.is_template()
            }
        }
    }
}

pub trait NodeType {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String>;
}

impl NodeType for NodeKindHandle {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String> {
        match self {
            NodeKindHandle::MatchV1(match_id) => match_set_preparation(ctx, *match_id),
            NodeKindHandle::CompetitionV1(c) => todo!(), //TODO we currently fail here.
            NodeKindHandle::MonitoringV1(_) => todo!(),
            NodeKindHandle::ServerV1(_) => todo!(),
            NodeKindHandle::ScheduleV1(_) => todo!(),
            NodeKindHandle::PortalV1(_) => todo!(),
            NodeKindHandle::RegistrationV1(_) => todo!(),
        }
    }
}
