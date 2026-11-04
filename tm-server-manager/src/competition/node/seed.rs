use std::collections::HashSet;

use spacetimedb::{
    CtxDbRead, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        node::{NodeHandle, NodeRead},
    },
    leaderboard::{
        LbEntry, LbParticipant, context::LbContext, error::LbError, modes::match_standings,
    },
    tm_match::{MatchRead, MatchStatus},
};

/// How the participants of the inputs of a node get their seeds.
#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeSeedOrder {
    /// Everyone of the first input, then everyone of the second one and so on.
    #[default]
    Concat,
    /// The first of every input, then the second of every input and so on.
    Interleave,
    /// By their places, the input with the lower slot wins a tie.
    ByPosition,
}

/// The seed order of a node, nodes without one use [`NodeSeedOrder::Concat`].
#[table(accessor = tab_node_seed_order, vis_private,
    index(accessor = node, hash(columns = [node_variant, node_id])))]
struct NodeSeedOrderSetting {
    node_variant: u8,
    node_id: u32,
    order: NodeSeedOrder,
}

/// The seed of a participant of a node.
/// Seeds get frozen when the node gets prepared, so they stay the same while it is played.
#[table(accessor = tab_node_seed, vis_private,
    index(accessor = node, hash(columns = [node_variant, node_id])))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeSeed {
    node_variant: u8,
    node_id: u32,
    /// 0 for the seed of a team.
    pub user_id: u32,
    pub team_id: u32,
    /// Starting at 1.
    pub seed: u16,
}

impl NodeSeed {
    fn new(node: NodeHandle, participant: LbParticipant, seed: u16) -> Self {
        let (node_variant, node_id) = node.split();
        let (user_id, team_id) = match participant {
            LbParticipant::Player(user_id) => (user_id, 0),
            LbParticipant::Team(team_id) => (0, team_id),
        };
        NodeSeed {
            node_variant,
            node_id,
            user_id,
            team_id,
            seed,
        }
    }
}

/// The participants of the inputs in seed order, someone in several inputs keeps the best seed.
fn seed_order(order: NodeSeedOrder, inputs: Vec<Vec<LbEntry>>) -> Vec<LbParticipant> {
    let ordered: Vec<LbEntry> = match order {
        NodeSeedOrder::Concat => inputs.into_iter().flatten().collect(),
        NodeSeedOrder::Interleave => {
            let longest = inputs.iter().map(Vec::len).max().unwrap_or(0);
            (0..longest)
                .flat_map(|place| {
                    inputs
                        .iter()
                        .filter_map(move |input| input.get(place).copied())
                })
                .collect()
        }
        NodeSeedOrder::ByPosition => {
            let mut entries: Vec<LbEntry> = inputs.into_iter().flatten().collect();
            // The sort is stable, so entries with the same place keep the order of the inputs.
            entries.sort_by_key(|e| e.position);
            entries
        }
    };

    let mut seen = HashSet::new();
    ordered
        .into_iter()
        .map(|e| e.participant())
        .filter(|participant| seen.insert(*participant))
        .collect()
}

/// The seeds in a new order, `order` lists the current seeds starting with the one which becomes the first.
fn reseeded(seeds: &[NodeSeed], order: &[u16]) -> Result<Vec<NodeSeed>, String> {
    if order.len() != seeds.len() {
        return Err(format!(
            "Expected all {} seeds in the new order but got {}.",
            seeds.len(),
            order.len()
        ));
    }

    let mut given = HashSet::new();
    order
        .iter()
        .zip(1u16..)
        .map(|(&current, seed)| {
            let Some(found) = seeds.iter().find(|s| s.seed == current) else {
                return Err(format!("Seed {current} does not exist."));
            };
            if !given.insert(current) {
                return Err(format!("Seed {current} is given more than once."));
            }
            Ok(NodeSeed { seed, ..*found })
        })
        .collect()
}

pub(crate) trait NodeSeedRead {
    fn node_seed_order(&self, node: NodeHandle) -> NodeSeedOrder;
    /// The seeds the node would get from its inputs right now.
    fn node_seeds_preview(&self, node: NodeHandle) -> Result<Vec<NodeSeed>, LbError>;
    /// The frozen seeds of the node, the best first.
    fn node_seeds(&self, node: NodeHandle) -> Vec<NodeSeed>;
}
impl<Db: CtxDbRead> NodeSeedRead for Db {
    fn node_seed_order(&self, node: NodeHandle) -> NodeSeedOrder {
        self.db_read_only()
            .tab_node_seed_order()
            .node()
            .filter(node.split())
            .next()
            .map(|setting| setting.order)
            .unwrap_or_default()
    }

    fn node_seeds_preview(&self, node: NodeHandle) -> Result<Vec<NodeSeed>, LbError> {
        let cx = LbContext::new(self, node);
        let inputs = self
            .node_resolve_inputs(node)?
            .into_iter()
            .map(|input| match_standings(&cx, input))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(seed_order(self.node_seed_order(node), inputs)
            .into_iter()
            .zip(1u16..)
            .map(|(participant, seed)| NodeSeed::new(node, participant, seed))
            .collect())
    }

    fn node_seeds(&self, node: NodeHandle) -> Vec<NodeSeed> {
        let mut seeds: Vec<NodeSeed> = self
            .db_read_only()
            .tab_node_seed()
            .node()
            .filter(node.split())
            .collect();
        seeds.sort_by_key(|s| s.seed);
        seeds
    }
}

pub(crate) trait NodeSeedWrite: NodeSeedRead {
    /// Freezes the seeds the node gets from its inputs, seeds which already got frozen are kept.
    fn node_seeds_freeze(&self, node: NodeHandle) -> Result<(), String>;
    fn node_seeds_delete(&self, node: NodeHandle);
}
impl<Db: spacetimedb::CtxDbWrite> NodeSeedWrite for Db {
    fn node_seeds_freeze(&self, node: NodeHandle) -> Result<(), String> {
        if !self.node_seeds(node).is_empty() {
            return Ok(());
        }
        for seed in self.node_seeds_preview(node)? {
            self.db().tab_node_seed().try_insert(seed)?;
        }
        Ok(())
    }

    fn node_seeds_delete(&self, node: NodeHandle) {
        self.db().tab_node_seed().node().delete(node.split());
        self.db().tab_node_seed_order().node().delete(node.split());
    }
}

#[reducer]
fn node_seed_order_update(
    ctx: &ReducerContext,
    node: NodeHandle,
    order: NodeSeedOrder,
) -> Result<(), String> {
    let parent = ctx.node_get_parent(node)?;
    ctx.auth_builder(parent)
        .permission(CompetitionPermissionsV1::NODE_SEEDING)
        .authorize()?;

    let (node_variant, node_id) = node.split();
    ctx.db
        .tab_node_seed_order()
        .node()
        .delete((node_variant, node_id));
    ctx.db.tab_node_seed_order().insert(NodeSeedOrderSetting {
        node_variant,
        node_id,
        order,
    });

    Ok(())
}

/// Reorders the frozen seeds of a match while it gets prepared.
/// `order` lists the current seeds starting with the one which becomes the first.
#[reducer]
fn node_reseed(ctx: &ReducerContext, node: NodeHandle, order: Vec<u16>) -> Result<(), String> {
    let parent = ctx.node_get_parent(node)?;
    ctx.auth_builder(parent)
        .permission(CompetitionPermissionsV1::NODE_SEEDING)
        .authorize()?;

    let NodeHandle::MatchV1(match_id) = node else {
        return Err("Only matches have frozen seeds.".into());
    };
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Err("Match not found.".into());
    };
    if tm_match.status() != MatchStatus::Preparation {
        return Err("Seeds can only be changed while the match gets prepared.".into());
    }

    let seeds = reseeded(&ctx.node_seeds(node), &order)?;
    ctx.db.tab_node_seed().node().delete(node.split());
    for seed in seeds {
        ctx.db.tab_node_seed().insert(seed);
    }

    Ok(())
}

fn is_member(ctx: &ViewContext, node: NodeHandle) -> bool {
    ctx.node_get_parent(node)
        .is_ok_and(|parent| ctx.auth_builder(parent).authorize().is_ok())
}

/// The frozen seeds of the node.
#[view(accessor = node_seeds, public)]
fn node_seeds(ctx: &ViewContext, node: NodeHandle) -> Vec<NodeSeed> {
    if !is_member(ctx, node) {
        return Vec::new();
    }
    ctx.node_seeds(node)
}

/// The seeds the node would get from its inputs right now.
#[view(accessor = node_seeds_preview, public)]
fn node_seeds_preview(ctx: &ViewContext, node: NodeHandle) -> Vec<NodeSeed> {
    if !is_member(ctx, node) {
        return Vec::new();
    }
    ctx.node_seeds_preview(node).unwrap_or_else(|error| {
        log::error!("The seeds of {node:?} could not be resolved: {error}");
        Vec::new()
    })
}
