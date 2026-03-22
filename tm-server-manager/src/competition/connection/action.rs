use spacetimedb::{ReducerContext, SpacetimeType, table};

use crate::{
    competition::node::NodeKindHandle,
    tm_match::{match_try_start, tab_match},
};

#[table(accessor=tab_connection_action)]
pub struct TabConnectionAction {
    #[index(hash)]
    competition_id: u32,
    #[primary_key]
    pub connection_id: u32,

    action: ConnectionAction,
}

impl TabConnectionAction {
    fn get_match(&self) -> ConnectionActionMatch {
        match self.action {
            ConnectionAction::MatchV1(connection_action_match) => connection_action_match,
            _ => unreachable!(),
        }
    }
}

// Versioning works be e.g.:
// MatchV1A2(ConnectionActionMatchV2)
#[derive(Debug, SpacetimeType)]
enum ConnectionAction {
    MatchV1(ConnectionActionMatch),
    RegistrationV1(ConnectionActionRegistration),
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum ConnectionActionMatch {
    TryStart,
    ForceStart,
}

#[derive(Debug, SpacetimeType)]
enum ConnectionActionRegistration {
    Open,
    Close,
}

pub(super) fn try_exec_action(connection: u32, target: NodeKindHandle, ctx: &ReducerContext) {
    let action = ctx
        .db
        .tab_connection_action()
        .connection_id()
        .find(connection)
        .unwrap();
    match target {
        NodeKindHandle::MatchV1(m) => {
            let match_action = action.get_match();
            match match_action {
                ConnectionActionMatch::TryStart => {
                    if let Err(error) = match_try_start(ctx, m) {
                        log::error!(
                            "Explicit Flow: match_try_start action failed through connection {} Error: {}",
                            connection,
                            error
                        );
                    }
                }
                ConnectionActionMatch::ForceStart => todo!(),
            }
        }
        NodeKindHandle::CompetitionV1(_) => unreachable!(),
        NodeKindHandle::MonitoringV1(_) => unreachable!(),
        NodeKindHandle::ServerV1(_) => unreachable!(),
        NodeKindHandle::ScheduleV1(_) => unreachable!(),
        NodeKindHandle::PortalV1(_) => unreachable!(),
        NodeKindHandle::RegistrationV1(r) => todo!(),
    }
}
