use std::collections::{HashMap, HashSet};

use spacetimedb::{
    AnonymousViewContext, Local, LocalReadOnly, ProcedureContext, ReducerContext, SpacetimeType,
    Uuid, procedure, reducer, sys::raw::volatile_nonatomic_schedule_immediate, view,
    volatile_nonatomic_schedule_immediate,
};
use tm_server_types::config::{ModeSettings, ModeSettingsV2, TmMode};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1, CompetitionRead, CompetitionWrite,
        connection::{
            ConnectionRead,
            action::tab_connection_action,
            data::{tab_connection_data, tab_connection_data__view},
            tab_connection, tab_connection__view,
        },
        roles::tab_competition_member__view,
        tab_competition, tab_competition__view,
    },
    input::{InputWrite, tab_input, tab_input__view},
    leaderboard::{
        LbEntry, LeadearboardRead, LeaderboardWrite, tab_leaderboard_v2, tab_leaderboard_v2__view,
    },
    output::{OutputWrite, tab_output, tab_output__view},
    raw_server::{config::RawServerContigRead, player::PermittedPlayer},
    registration::{
        RegistrationWrite, player::RegistrationRead, tab_registration, tab_registration__view,
    },
    schedule::{ScheduleWrite, tab_schedule, tab_schedule__view},
    tm_match::{MatchWrite, leaderboard::MatchLeadearboardRead, tab_match, tab_match__view},
    tm_server::{ServerWrite, tab_server, tab_server__view},
    user::UserRead,
};
mod position;

pub use position::*;

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
            NodeHandle::ServerV1(n) => {
                let node = ctx.db.tab_server().id().find(n).unwrap();
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
    }

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
        .permission(CompetitionPermissionsV1::OWNER)
        .authorize()?;

    ctx.node_delete(node)
}

pub(crate) trait NodeRead {
    fn node_permitted_players_input(&self, node: NodeHandle) -> Vec<PermittedPlayer>;
    fn node_get_parent(&self, node: NodeHandle) -> Result<u32, String>;
    fn node_resolve_input_data(&self, node: NodeHandle) -> Vec<LbEntry>;
    fn node_resolve_output_data(&self, node: NodeHandle) -> Vec<LbEntry>;
}
impl<Db: spacetimedb::CtxDbRead> NodeRead for Db {
    fn node_permitted_players_input(&self, node: NodeHandle) -> Vec<PermittedPlayer> {
        let mut map: HashMap<Uuid, PermittedPlayer> = HashMap::new();

        let parent = self.node_get_parent(node).unwrap();

        let tree = self.competition_ancestors(parent);
        for comp in tree {
            map.extend(
                self.db_read_only()
                    .tab_competition_member()
                    .competition_id()
                    .filter(comp)
                    .filter_map(|m| {
                        if m.get_permissions()
                            .has(CompetitionPermissionsV1::TRACKMANIA_SPECTATE_MATCHES)
                        {
                            let account_id = self.user_account_from_id(m.user());
                            return Some((
                                account_id,
                                PermittedPlayer::new(account_id, false, true),
                            ));
                        }
                        None
                    }),
            )
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
            .finalize(self)
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
                if let Some(ma) = self.db_read_only().tab_match().id().find(m) {
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
            NodeHandle::ScheduleV1(s) => {
                if let Some(ma) = self.db_read_only().tab_schedule().id().find(s) {
                    Ok(ma.parent_id())
                } else {
                    Err("Schedule could not be found.".into())
                }
            }
            NodeHandle::ServerV1(s) => {
                if let Some(ma) = self.db_read_only().tab_server().id().find(s) {
                    Ok(ma.parent_id())
                } else {
                    Err("Server could not be found.".into())
                }
            }
            NodeHandle::RegistrationV1(reg) => {
                if let Some(reg) = self.db_read_only().tab_registration().id().find(reg) {
                    Ok(reg.get_comp_id())
                } else {
                    Err("Registration could not be found.".into())
                }
            }
            NodeHandle::InputV1(node) => {
                if let Some(node) = self.db_read_only().tab_input().id().find(node) {
                    Ok(node.get_comp_id())
                } else {
                    Err("Registration could not be found.".into())
                }
            }
            NodeHandle::OutputV1(node) => {
                if let Some(node) = self.db_read_only().tab_output().id().find(node) {
                    Ok(node.get_comp_id())
                } else {
                    Err("Registration could not be found.".into())
                }
            }
            NodeHandle::LeaderboardV1(node) => {
                if let Some(node) = self.db_read_only().tab_leaderboard_v2().id().find(node) {
                    Ok(node.get_comp_id())
                } else {
                    Err("Registration could not be found.".into())
                }
            }
        }
    }

    fn node_resolve_input_data(&self, node: NodeHandle) -> Vec<LbEntry> {
        node_resolve_input_data_inner(self, node, &mut 1)
    }

    fn node_resolve_output_data(&self, node: NodeHandle) -> Vec<LbEntry> {
        node_resolve_output_data_inner(self, node, &mut 1)
    }
}

fn node_resolve_output_data_inner(
    ctx: &impl spacetimedb::CtxDbRead,
    node: NodeHandle,
    origin_offset: &mut u8,
) -> Vec<LbEntry> {
    match node {
        NodeHandle::ScheduleV1(n) => {
            log::warn!("Attempted to read node data output for a schedule node: {n}");
            Vec::new()
        }
        NodeHandle::ServerV1(n) => {
            log::warn!("Attempted to read node data output for a server node: {n}");
            Vec::new()
        }
        NodeHandle::MatchV1(n) => ctx.match_rounds(n),
        NodeHandle::CompetitionV1(n) => {
            let Some(output) = ctx.db_read_only().tab_output().parent_id().filter(n).next() else {
                log::warn!("Requested lb for competition {n} but it has no output node!");
                return Vec::new();
            };

            node_resolve_input_data_inner(ctx, NodeHandle::OutputV1(output.id), origin_offset)
        }
        NodeHandle::OutputV1(n) => {
            node_resolve_input_data_inner(ctx, NodeHandle::OutputV1(n), origin_offset)
        }
        NodeHandle::InputV1(n) => {
            let Some(input) = ctx.db_read_only().tab_input().id().find(n) else {
                return Vec::new();
            };
            let comp = input.get_comp_id();

            node_resolve_input_data_inner(ctx, NodeHandle::CompetitionV1(comp), origin_offset)
        }
        NodeHandle::RegistrationV1(n) => ctx.registration_lb(n),
        NodeHandle::LeaderboardV1(n) => ctx.leaderboard_evaluation(n),
    }
}

fn node_resolve_input_data_inner(
    ctx: &impl spacetimedb::CtxDbRead,
    node: NodeHandle,
    origin_offset: &mut u8,
) -> Vec<LbEntry> {
    let mut player_entries = Vec::new();

    let depending_connections = ctx
        .db_read_only()
        .tab_connection()
        .origins_of()
        .filter(node.split())
        .filter(|c| c.is_data());

    for depending_connection in depending_connections {
        let lb_entries =
            node_resolve_output_data_inner(ctx, depending_connection.origin(), origin_offset);

        let filtered = ctx.connection_resolve_leaderboard(depending_connection, lb_entries);

        player_entries.extend(filtered);
    }

    player_entries
}

pub(crate) trait NodeWrite: NodeRead {
    fn node_create(&self, node: NodeHandle, position: Vec2) -> Result<(), String>;
    fn node_delete(&self, node: NodeHandle) -> Result<(), String>;
    fn connection_delete(&self, connection_id: u32) -> Result<(), String>;
    fn node_name_edit(&self, node: NodeHandle, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> NodeWrite for Db {
    fn node_create(&self, node: NodeHandle, position: Vec2) -> Result<(), String> {
        self.node_position_insert(node,position)?;

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

    fn connection_delete(&self, connection_id: u32) -> Result<(), String> {
        let Some(connection) = self.db().tab_connection().id().find(connection_id) else {
            return Err("Connection not found".into());
        };

        if connection.is_action() {
            self.db()
                .tab_connection_action()
                .connection_id()
                .delete(connection.id);
        }
        if connection.is_data() {
            self.db()
                .tab_connection_data()
                .connection_id()
                .delete(connection.id);
        }

        self.db().tab_connection().id().delete(connection.id);

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
    //TODO access control

    ctx.node_name_edit(node, name)
}

/* #[view(accessor=unstable_dw_test_permit_players,public)]
fn unstable_dw_test_permit_players(ctx: &AnonymousViewContext) -> Vec<PermittedPlayer> {
    ctx.node_permitted_players_input(NodeHandle::MatchV1(12293))
}

#[view(accessor=unstable_dw_test_permit_players_2,public)]
fn unstable_dw_test_permit_players_2(ctx: &AnonymousViewContext) -> Vec<PermittedPlayer> {
    ctx.node_permitted_players_input(NodeHandle::MatchV1(12294))
} */

#[procedure]
fn node_leaderboard_output(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_resolve_output_data(node).finalize(ctx)))
}

#[procedure]
fn node_leaderboard_output_raw(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_resolve_output_data(node)))
}

#[procedure]
fn node_leaderboard_input(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_resolve_input_data(node)))
}

// To test the destination
#[procedure]
fn node_leaderboard_input_finalize(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<LbEntry>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_resolve_input_data(node).finalize(ctx)))
}

// To test the destination
#[procedure]
fn test_node_permitted_players_input(
    ctx: &mut ProcedureContext,
    node: NodeHandle,
) -> Result<Vec<PermittedPlayer>, String> {
    ctx.try_with_tx(|ctx| Ok(ctx.node_permitted_players_input(node)))
}

pub trait NodeLeaderboard {
    fn finalize(&self, ctx: &impl spacetimedb::CtxDbRead) -> Vec<LbEntry>;
}

impl NodeLeaderboard for Vec<LbEntry> {
    fn finalize(&self, ctx: &impl spacetimedb::CtxDbRead) -> Vec<LbEntry> {
        /*  let Some(state) = self
            .db_read_only()
            .tab_match_state()
            .match_id()
            .find(match_id)
        else {
            return Vec::new();
        };
        if round == 0 {
            round = state.get_round();
        };
        let mut entries: Vec<MatchRoundPlayer> = self
            .db_read_only()
            .tab_match_round_player()
            .match_round_range()
            .filter((match_id, 1..=round))
            .collect(); */

        let mut entries = self.clone();

        let mut map = HashMap::<u32, LbEntry>::new();

        let Some(entry) = entries.get(0) else {
            return Vec::new();
        };
        let entry = *entry;

        let returned = match entry.get_mode() {
            TmMode::Rounds => {
                for entry in entries {
                    map.entry(entry.user_id)
                        .and_modify(|e| {
                            e.score += entry.score;
                            if entry.round > e.round {
                                e.round = entry.round;
                            }
                        })
                        .or_insert(entry);
                }

                let mut standings = map.into_values().collect::<Vec<_>>();

                standings.sort_by_key(|v| -v.score);

                for (index, stand) in standings.iter_mut().enumerate() {
                    stand.position = (index + 1) as u16;
                }

                standings
            }
            TmMode::ReverseCup => {
                let mut eliminated = HashSet::new();

                for entry in entries {
                    map.entry(entry.user_id)
                        .and_modify(|e| {
                            if entry.score <= -2000 {
                                eliminated.insert(e.user_id);
                            }
                            e.score += entry.score;

                            if entry.round > e.round {
                                e.round = entry.round;
                            }
                        })
                        .or_insert(entry);
                }

                let mut standings = map.into_values().collect::<Vec<_>>();

                let tm_match = ctx
                    .db_read_only()
                    .tab_match()
                    .id()
                    .find(entry.get_node().id())
                    .unwrap();
                let cfg = ctx.raw_server_config(tm_match.get_config_id()).unwrap();
                let starting_points = match cfg.get_mode() {
                    ModeSettingsV2::ReverseCup(reverse_cup) => reverse_cup.starting_points,
                    _ => unreachable!(),
                };

                for stand in standings.iter_mut() {
                    stand.score += starting_points;

                    if !eliminated.contains(&stand.user_id) || stand.score > 0 {
                        stand.round += 1;
                    }
                }

                standings.sort_by_key(|v| -(v.round as i32));

                for (index, stand) in standings.iter_mut().enumerate() {
                    stand.position = (index + 1) as u16;
                }

                standings
            }
            TmMode::Knockout => {
                for entry in entries {
                    map.entry(entry.user_id)
                        .and_modify(|e| {
                            if entry.score == -1 {
                                *e = entry;
                            }

                            if entry.round > e.round {
                                e.round = entry.round;
                            }
                        })
                        .or_insert(entry);
                }

                let mut standings = map.into_values().collect::<Vec<_>>();

                standings.sort_by_key(|v| -(v.round as i32));

                for (index, stand) in standings.iter_mut().enumerate() {
                    stand.position = (index + 1) as u16;
                }

                standings
            }
            TmMode::TimeAttack => {
                entries.sort_by_key(|v| if v.time <= 0 { i32::MAX } else { v.time });

                entries
            }
            TmMode::Unknown => {
                // should come out of lb node so its always flattened in our case rn TODO fix this up.
                entries.sort_by_key(|k| k.position);
                entries
            }
        };

        // log::info!("{returned:?}");

        returned
    }
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
