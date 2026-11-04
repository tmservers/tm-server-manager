use std::collections::HashMap;

use spacetimedb::{ProcedureContext, ReducerContext, SpacetimeType, Uuid, procedure, reducer};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1, CompetitionRead, CompetitionWrite,
        connection::{ConnectionRead, ConnectionWrite, data::ConnectionDataRead},
        roles::CompetitionMemberRead,
        tab_competition__view,
    },
    input::{InputRead, InputWrite},
    leaderboard::{
        LbEntry, LeadearboardRead, LeaderboardWrite, context::LbContext, error::LbError,
        modes::match_standings, shape::LbShape,
    },
    output::{OutputRead, OutputWrite},
    raw_server::player::PermittedPlayer,
    registration::{RegistrationRead, RegistrationWrite, player::RegistrationPlayerRead},
    schedule::{ScheduleRead, ScheduleWrite},
    tm_match::{MatchRead, MatchStatus, MatchWrite, leaderboard::MatchLeadearboardRead},
    tm_server::{ServerRead, ServerWrite},
    user::UserRead,
};
mod position;
pub(crate) mod seed;

pub use position::*;

use seed::NodeSeedWrite;

#[derive(Debug, PartialEq, Eq, Clone, Copy, SpacetimeType, Hash)]
#[non_exhaustive]
pub enum NodeHandle {
    MatchV1(u32),
    CompetitionV1(u32),
    ScheduleV1(u32),
    ServerV1(u32),
    InputV1(u32),
    OutputV1(u32),
    RegistrationV1(u32),
    LeaderboardV1(u32),
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
            NodeHandle::ServerV1(m) => (4, m),
            NodeHandle::InputV1(h) => (5, h),
            NodeHandle::OutputV1(h) => (6, h),
            NodeHandle::RegistrationV1(r) => (7, r),
            NodeHandle::LeaderboardV1(l) => (8, l),
        }
    }

    pub(crate) fn combine(variant: u8, value: u32) -> Self {
        match variant {
            1 => Self::MatchV1(value),
            2 => Self::CompetitionV1(value),
            3 => Self::ScheduleV1(value),
            4 => Self::ServerV1(value),
            5 => Self::InputV1(value),
            6 => Self::OutputV1(value),
            7 => Self::RegistrationV1(value),
            8 => Self::LeaderboardV1(value),
            _ => unreachable!(),
        }
    }

    /* pub(crate) fn is_template(&self, ctx: &ReducerContext) -> bool {
        match self {
            NodeHandle::MatchV1(m) => {
                let node = ctx.match_find(*m).unwrap();
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
            NodeHandle::ServerV1(n) => {
                let node = ctx.server_find(*n).unwrap();
                node.is_template()
            }
            NodeHandle::RegistrationV1(reg) => {
                let node = ctx.db.tab_registration().id().find(reg).unwrap();
                node.is_template()
            }
            NodeHandle::InputV1(n) => {
                let node = ctx.db.tab_input().id().find(n).unwrap();
                node.is_template()
            }
            NodeHandle::OutputV1(n) => {
                let node = ctx.db.tab_output().id().find(n).unwrap();
                node.is_template()
            }
            NodeHandle::LeaderboardV1(n) => {
                let node = ctx.db.tab_leaderboard_v2().id().find(n).unwrap();
                node.is_template()
            }
        }
    } */

    pub(crate) fn is_match(&self) -> bool {
        matches!(self, NodeHandle::MatchV1(_))
    }
    pub(crate) fn is_server(&self) -> bool {
        matches!(self, NodeHandle::ServerV1(_))
    }
    pub(crate) fn is_input(&self) -> bool {
        matches!(self, NodeHandle::InputV1(_))
    }
    pub(crate) fn is_output(&self) -> bool {
        matches!(self, NodeHandle::OutputV1(_))
    }
    pub(crate) fn is_competition(&self) -> bool {
        matches!(self, NodeHandle::CompetitionV1(_))
    }
    pub(crate) fn is_registration(&self) -> bool {
        matches!(self, NodeHandle::RegistrationV1(_))
    }
    pub(crate) fn is_schedule(&self) -> bool {
        matches!(self, NodeHandle::ScheduleV1(_))
    }
    pub(crate) fn is_leaderboard(&self) -> bool {
        matches!(self, NodeHandle::LeaderboardV1(_))
    }

    pub(crate) fn id(&self) -> u32 {
        self.split().1
    }
}

/* pub trait NodeType {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String>;
}

impl NodeType for NodeHandle {
    fn ready(&self, ctx: &ReducerContext) -> Result<(), String> {
        match self {
            NodeHandle::MatchV1(match_id) => ctx.match_set_preparation(*match_id, ctx.timestamp),
            NodeHandle::CompetitionV1(c) => todo!(), // trigger the input node.
            NodeHandle::ServerV1(_) => todo!(),
            NodeHandle::ScheduleV1(s) => ctx.schedule_start_relative(*s, ctx.timestamp),
            NodeHandle::RegistrationV1(r) => unreachable!(),
            NodeHandle::InputV1(_) => todo!(),
            NodeHandle::OutputV1(_) => todo!(),
            // Nothing should really happen here because the leaderboard will recompute itself.
            NodeHandle::LeaderboardV1(_) => Ok(()),
        }
    }
} */

#[reducer]
fn node_delete(ctx: &ReducerContext, node: NodeHandle) -> Result<(), String> {
    let parent = ctx.node_get_parent(node)?;
    ctx.auth_builder(parent)
        .permission(CompetitionPermissionsV1::NODE_DELETE)
        .authorize()?;

    ctx.node_delete(node)
}

pub(crate) trait NodeRead {
    fn node_permitted_players_input(&self, node: NodeHandle) -> Vec<PermittedPlayer>;
    fn node_get_parent(&self, node: NodeHandle) -> Result<u32, String>;
    fn node_resolve_input_data(&self, node: NodeHandle) -> Result<Vec<LbEntry>, LbError>;
    /// The entries of every data connection into the node on their own, ordered by slot.
    fn node_resolve_inputs(&self, node: NodeHandle) -> Result<Vec<Vec<LbEntry>>, LbError>;
    fn node_resolve_output_data(&self, node: NodeHandle) -> Result<Vec<LbEntry>, LbError>;
    /// What the node would get as input, the steps on the way are checked.
    fn node_input_shape(&self, node: NodeHandle) -> Result<LbShape, LbError>;
    /// What the node would output, the steps on the way are checked.
    fn node_output_shape(&self, node: NodeHandle) -> Result<LbShape, LbError>;
    /// The nodes this node still waits for, action connections do not count.
    fn node_pending_origins(&self, node: NodeHandle) -> Vec<NodeHandle>;
    /// The nodes which already took what this node passes on, action connections do not count.
    fn node_dependents(&self, node: NodeHandle) -> Vec<NodeHandle>;
    /// Whether the node is done, so what it outputs does not change anymore.
    fn node_is_finished(&self, node: NodeHandle) -> bool;
    fn node_is_template(&self, node: NodeHandle) -> bool;
}
impl<Db: spacetimedb::CtxDbRead> NodeRead for Db {
    fn node_permitted_players_input(&self, node: NodeHandle) -> Vec<PermittedPlayer> {
        let mut map: HashMap<Uuid, PermittedPlayer> = HashMap::new();

        let parent = self.node_get_parent(node).unwrap();

        let tree = self.competition_ancestors(parent);
        for comp in tree {
            map.extend(self.competition_members(comp).into_iter().filter_map(|m| {
                if m.get_permissions()
                    .has(CompetitionPermissionsV1::TRACKMANIA_SPECTATE_MATCHES)
                {
                    let account_id = self.user_account_from_id(m.user());
                    return Some((account_id, PermittedPlayer::new(account_id, false, true)));
                }
                None
            }))
        }
        /* let depending_connections = self
            .db_read_only()
            .tab_connection()
            .origins_of()
            .filter(node.split())
            .filter(|c| c.is_data());
        for depending_connection in depending_connections {
            let permitted_players = self
                .connection_filter_permitted_players(depending_connection)
                .into_iter()
                // This overrides the existing entrys.
            } */
        let players = self
            .node_resolve_input_data(node)
            .and_then(|input| match_standings(&LbContext::new(self, node), input))
            .unwrap_or_else(|error| {
                log::error!("The permitted players of {node:?} could not be resolved: {error}");
                Vec::new()
            })
            .into_iter()
            .map(|p| {
                let account_id = self.user_account_from_id(p.get_user());
                (account_id, PermittedPlayer::new(account_id, false, false))
            });
        map.extend(players);

        map.into_values().collect()
    }

    fn node_get_parent(&self, node: NodeHandle) -> Result<u32, String> {
        match node {
            NodeHandle::MatchV1(m) => {
                if let Some(ma) = self.match_find(m) {
                    Ok(ma.get_comp_id())
                } else {
                    Err("Match couldnt be found.".into())
                }
            }
            NodeHandle::CompetitionV1(c) => {
                if let Some(co) = self.db_read_only().tab_competition().id().find(c) {
                    let id = co.parent_id;
                    if id != 0 {
                        Ok(id)
                    } else {
                        Err("Compeittion without Parent cannot be part of a connection".into())
                    }
                } else {
                    Err("Competition could not be found".into())
                }
            }
            NodeHandle::ScheduleV1(s) => Ok(self.schedule_find(s)?.parent_id()),
            NodeHandle::ServerV1(s) => Ok(self.server_find(s)?.parent_id()),
            NodeHandle::RegistrationV1(reg) => Ok(self.registration_find(reg)?.get_comp_id()),
            NodeHandle::InputV1(node) => Ok(self.input_find(node)?.get_comp_id()),
            NodeHandle::OutputV1(node) => Ok(self.output_find(node)?.get_comp_id()),
            NodeHandle::LeaderboardV1(node) => Ok(self.leaderboard_find(node)?.get_comp_id()),
        }
    }

    fn node_resolve_input_data(&self, node: NodeHandle) -> Result<Vec<LbEntry>, LbError> {
        node_resolve_input_data_inner(self, node, &mut 1)
    }

    fn node_resolve_inputs(&self, node: NodeHandle) -> Result<Vec<Vec<LbEntry>>, LbError> {
        node_resolve_inputs_inner(self, node, &mut 1)
    }

    fn node_resolve_output_data(&self, node: NodeHandle) -> Result<Vec<LbEntry>, LbError> {
        node_resolve_output_data_inner(self, node, &mut 1)
    }

    fn node_input_shape(&self, node: NodeHandle) -> Result<LbShape, LbError> {
        self.connection_origins_of(node)
            .filter(|c| c.is_data())
            .try_fold(LbShape::default(), |shape, connection| {
                let output = self.node_output_shape(connection.origin())?;
                Ok(shape.combine(self.connection_shape(connection, output)?))
            })
    }

    fn node_output_shape(&self, node: NodeHandle) -> Result<LbShape, LbError> {
        match node {
            NodeHandle::ScheduleV1(_) | NodeHandle::ServerV1(_) => Ok(LbShape::default()),
            NodeHandle::MatchV1(_) => Ok(LbShape::rounds(node)),
            NodeHandle::CompetitionV1(n) => match self.outputs_in_parent(n).next() {
                Some(output) => self.node_input_shape(NodeHandle::OutputV1(output.id)),
                None => Ok(LbShape::default()),
            },
            NodeHandle::OutputV1(_) => self.node_input_shape(node),
            NodeHandle::InputV1(n) => match self.input_find(n) {
                Ok(input) => self.node_input_shape(NodeHandle::CompetitionV1(input.get_comp_id())),
                Err(_) => Ok(LbShape::default()),
            },
            NodeHandle::RegistrationV1(_) => Ok(LbShape::standings(node)),
            NodeHandle::LeaderboardV1(n) => self.leaderboard_output_shape(n),
        }
    }

    fn node_pending_origins(&self, node: NodeHandle) -> Vec<NodeHandle> {
        self.connection_origins_of(node)
            .filter(|c| !c.is_action() && !c.is_resolved())
            .map(|c| c.origin())
            .collect()
    }

    fn node_dependents(&self, node: NodeHandle) -> Vec<NodeHandle> {
        self.connection_targets_of(node)
            .filter(|c| !c.is_action() && c.is_resolved())
            .map(|c| c.target())
            .collect()
    }

    fn node_is_finished(&self, node: NodeHandle) -> bool {
        match node {
            NodeHandle::MatchV1(m) => self
                .match_find(m)
                .is_some_and(|m| m.status() == MatchStatus::Ended),
            NodeHandle::ScheduleV1(s) => self.schedule_find(s).is_ok_and(|s| s.is_finished()),
            NodeHandle::RegistrationV1(r) => self.registration_find(r).is_ok_and(|r| r.has_ended()),
            // These pass on what they get, so they are done once everything before them is.
            NodeHandle::LeaderboardV1(l) => {
                self.leaderboard_find(l).is_ok_and(|l| l.is_configured())
                    && self.node_pending_origins(node).is_empty()
            }
            NodeHandle::OutputV1(_) => self.node_pending_origins(node).is_empty(),
            NodeHandle::CompetitionV1(c) => self
                .outputs_in_parent(c)
                .next()
                .is_some_and(|output| self.node_is_finished(NodeHandle::OutputV1(output.id))),
            NodeHandle::InputV1(i) => self.input_find(i).is_ok_and(|input| {
                self.node_pending_origins(NodeHandle::CompetitionV1(input.get_comp_id()))
                    .is_empty()
            }),
            NodeHandle::ServerV1(_) => false,
        }
    }

    fn node_is_template(&self, node: NodeHandle) -> bool {
        match node {
            NodeHandle::MatchV1(m) => {
                let node = self.match_find(m).unwrap();
                node.is_template()
            }
            NodeHandle::CompetitionV1(c) => {
                let node = self.competition_find(c).unwrap();
                node.is_template()
            }
            NodeHandle::ScheduleV1(s) => {
                let node = self.schedule_find(s).unwrap();
                node.is_template()
            }
            NodeHandle::ServerV1(n) => {
                let node = self.server_find(n).unwrap();
                node.is_template()
            }
            NodeHandle::RegistrationV1(reg) => {
                let node = self.registration_find(reg).unwrap();
                node.is_template()
            }
            NodeHandle::InputV1(n) => {
                let node = self.input_find(n).unwrap();
                node.is_template()
            }
            NodeHandle::OutputV1(n) => {
                let node = self.output_find(n).unwrap();
                node.is_template()
            }
            NodeHandle::LeaderboardV1(n) => {
                let node = self.leaderboard_find(n).unwrap();
                node.is_template()
            }
        }
    }
}

fn node_resolve_output_data_inner(
    ctx: &impl spacetimedb::CtxDbRead,
    node: NodeHandle,
    origin_offset: &mut u8,
) -> Result<Vec<LbEntry>, LbError> {
    match node {
        NodeHandle::ScheduleV1(n) => {
            log::warn!("Attempted to read node data output for a schedule node: {n}");
            Ok(Vec::new())
        }
        NodeHandle::ServerV1(n) => {
            log::warn!("Attempted to read node data output for a server node: {n}");
            Ok(Vec::new())
        }
        NodeHandle::MatchV1(n) => Ok(ctx.match_rounds(n)),
        NodeHandle::CompetitionV1(n) => {
            let Some(output) = ctx.outputs_in_parent(n).next() else {
                log::warn!("Requested lb for competition {n} but it has no output node!");
                return Ok(Vec::new());
            };

            node_resolve_input_data_inner(ctx, NodeHandle::OutputV1(output.id), origin_offset)
        }
        NodeHandle::OutputV1(n) => {
            node_resolve_input_data_inner(ctx, NodeHandle::OutputV1(n), origin_offset)
        }
        NodeHandle::InputV1(n) => {
            let Ok(input) = ctx.input_find(n) else {
                return Ok(Vec::new());
            };
            let comp = input.get_comp_id();

            node_resolve_input_data_inner(ctx, NodeHandle::CompetitionV1(comp), origin_offset)
        }
        NodeHandle::RegistrationV1(n) => Ok(ctx.registration_lb(n)),
        NodeHandle::LeaderboardV1(n) => ctx.leaderboard_evaluation(n),
    }
}

fn node_resolve_input_data_inner(
    ctx: &impl spacetimedb::CtxDbRead,
    node: NodeHandle,
    origin_offset: &mut u8,
) -> Result<Vec<LbEntry>, LbError> {
    Ok(node_resolve_inputs_inner(ctx, node, origin_offset)?
        .into_iter()
        .flatten()
        .collect())
}

fn node_resolve_inputs_inner(
    ctx: &impl spacetimedb::CtxDbRead,
    node: NodeHandle,
    origin_offset: &mut u8,
) -> Result<Vec<Vec<LbEntry>>, LbError> {
    let mut depending_connections: Vec<_> = ctx
        .connection_origins_of(node)
        .filter(|c| c.is_data())
        .collect();
    depending_connections.sort_by_cached_key(|c| {
        let slot = ctx.connection_data_find(c.id).map_or(0, |data| data.slot());
        (slot, c.id)
    });

    depending_connections
        .into_iter()
        .map(|depending_connection| {
            let lb_entries =
                node_resolve_output_data_inner(ctx, depending_connection.origin(), origin_offset)?;
            ctx.connection_resolve_leaderboard(depending_connection, lb_entries)
        })
        .collect()
}

pub(crate) trait NodeWrite: NodeRead {
    fn node_create(&self, node: NodeHandle, position: Vec2) -> Result<(), String>;
    fn node_delete(&self, node: NodeHandle) -> Result<(), String>;
    fn node_name_edit(&self, node: NodeHandle, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> NodeWrite for Db {
    fn node_create(&self, node: NodeHandle, position: Vec2) -> Result<(), String> {
        self.node_position_insert(node, position)?;

        Ok(())
    }

    fn node_delete(&self, node: NodeHandle) -> Result<(), String> {
        //TODO also delete the node itself with all component tables and checks for allowed
        /* match node {
            NodeHandle::MatchV1(n) => todo!(),
            NodeHandle::CompetitionV1(n) => todo!(),
            NodeHandle::ScheduleV1(_) => todo!(),
            NodeHandle::ServerV1(_) => todo!(),
            NodeHandle::InputV1(_) => todo!(),
            NodeHandle::OutputV1(_) => todo!(),
            NodeHandle::RegistrationV1(_) => todo!(),
            NodeHandle::LeaderboardV1(_) => todo!(),
        } */

        let dependents = self.node_dependents(node);
        if !dependents.is_empty() {
            return Err(format!(
                "The node cannot be deleted anymore because {dependents:?} already rely on it."
            ));
        }

        // Delete postition table entry.
        self.node_position_delete(node);

        self.node_seeds_delete(node);

        // Delete all associated connection tables involving the node.
        self.node_connections_delete(node);

        Ok(())
    }

    fn node_name_edit(&self, node: NodeHandle, name: String) -> Result<(), String> {
        match node {
            NodeHandle::MatchV1(id) => self.match_name_edit(id, name)?,
            NodeHandle::CompetitionV1(id) => self.competition_name_edit(id, name)?,
            NodeHandle::ServerV1(id) => self.server_name_edit(id, name)?,
            NodeHandle::ScheduleV1(id) => self.schedule_name_edit(id, name)?,
            NodeHandle::RegistrationV1(id) => self.registration_name_edit(id, name)?,
            NodeHandle::InputV1(id) => self.input_name_edit(id, name)?,
            NodeHandle::OutputV1(id) => self.output_name_edit(id, name)?,
            NodeHandle::LeaderboardV1(id) => self.leaderboard_name_edit(id, name)?,
        }

        Ok(())
    }
}

#[reducer]
fn node_name_edit(ctx: &ReducerContext, node: NodeHandle, name: String) -> Result<(), String> {
    let parent = ctx.node_get_parent(node)?;
    ctx.auth_builder(parent)
        .permission(CompetitionPermissionsV1::NODE_NAME_EDIT)
        .authorize()?;

    ctx.node_name_edit(node, name)
}

#[procedure]
fn node_leaderboard_output(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| {
        let output = ctx.node_resolve_output_data(node)?;
        Ok(match_standings(&LbContext::new(ctx, node), output)?)
    })
}

#[procedure]
fn node_leaderboard_output_raw(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_resolve_output_data(node)?))
}

#[procedure]
fn node_leaderboard_input(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_resolve_input_data(node)?))
}

// To test the destination
#[procedure]
fn node_leaderboard_input_finalize(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| {
        let input = ctx.node_resolve_input_data(node)?;
        Ok(match_standings(&LbContext::new(ctx, node), input)?)
    })
}

// To test the destination
#[procedure]
fn test_node_permitted_players_input(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<PermittedPlayer>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_permitted_players_input(node)))
}

/*
Mode -> Opinionated sorting

Origin -> Lb | Match | the compe/in/out but only Proxy

Connection Filter -> Only on "pure" lb -> each Player one time occurence

Lb Node -> Can operate on pure and impure lb -> Always get the full data and assemble it yourself… -> how is this possible with Connection filters? -> could cf purify the lb? -> probably Need to get rid of the matchroundplayer struct all together and immediately downcast to lb.

We have two possible Solutions here: have some data duplication and meaningless columns in certain situations.
Make Dedicated structs for each Scenario :(


Maybe there is a further Option where we can use an enum to dispatch between those possible situations -> improves type safety and Client side filtering.

The purify function is VERY tricky

but a unified way to filter EVERYTHNG would make the codebase infinetly better -> mayyybe get rid of the data connection

*/
