use spacetimedb::{
    DbContext, Local, Query, ReducerContext, SpacetimeType, Table, ViewContext, reducer, view,
};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        node::{NodeHandle, NodeRead},
    },
};

#[spacetimedb::table(
    accessor= tab_competition_node_position,
    index(accessor=node_position,hash(columns=[node_variant,node_id])),
    index(accessor=temp_competition_id,btree(columns=[competition_id]))
)]
#[derive(Debug, Clone, Copy)]
struct TabCompetitionNodePosition {
    position: Vec2,
    // This is just so that we can update it.
    // Rework if multi col unique indices are there.
    #[auto_inc]
    #[primary_key]
    id: u32,

    #[index(hash)]
    competition_id: u32,

    node_id: u32,
    node_variant: u8,
}

impl TabCompetitionNodePosition {
    pub(crate) fn new(node: NodeHandle, competition_id: u32) -> Self {
        let (node_variant, node_id) = node.split();
        Self {
            id: 0,
            competition_id,
            node_id,
            node_variant,
            position: Vec2 { x: 0., y: 0. },
        }
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub struct Vec2 {
    x: f32,
    y: f32,
}

#[derive(Debug, SpacetimeType)]
pub struct CompetitionNodePosition {
    competition_id: u32,
    node: NodeHandle,

    position: Vec2,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub struct NodePositionUpdate {
    node: NodeHandle,
    position: Vec2,
}

/* #[view(accessor=competition_node_position,public)]
fn competition_node_position(
    ctx: &ViewContext, /* competition_id: u32 */
) -> Vec<CompetitionNodePosition> {
    let competition_id = 1u32;
    ctx.db
        .tab_competition_node_position()
        .competition_id()
        .filter(competition_id)
        .map(|v| CompetitionNodePosition {
            competition_id: v.competition_id,
            node: NodeHandle::combine(v.node_variant, v.node_id),
            position: v.position,
        })
        .collect()
} */

#[view(accessor=my_node_positions,public)]
fn my_node_positions(ctx: &ViewContext, /* competition_id: u32 */) -> Vec<CompetitionNodePosition> {
    /* let Ok(user) = ctx.user_id() else {
        log::warn!(
            "Non user account has tried to call protected view: {}",
            ctx.sender()
        );
        return Vec::new();
    }; */

    let competition_id = 1u32;

    //TODO access control for only permitted users. e.g. walk competition tree for permission.

    //TODO switch to the arg and a hash index
    ctx.db
        .tab_competition_node_position()
        .temp_competition_id()
        .filter(competition_id..u32::MAX)
        .map(|v| CompetitionNodePosition {
            competition_id: v.competition_id,
            node: NodeHandle::combine(v.node_variant, v.node_id),
            position: v.position,
        })
        .collect()
}

#[reducer]
fn competition_node_position_update(
    ctx: &ReducerContext,
    node: NodeHandle,
    position: Vec2,
) -> Result<(), String> {
    let competition_id = ctx.node_get_parent(node)?;
    ctx.auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::COMPETITION_LAYOUT_EDIT)
        .authorize()?;

    let Some(mut node) = ctx
        .db
        .tab_competition_node_position()
        .node_position()
        .filter(node.split())
        .next()
    else {
        return Err("Couldnt find node".into());
    };

    node.position = position;

    ctx.db.tab_competition_node_position().id().update(node);

    Ok(())
}

// Update multiple node positions at once.
#[reducer]
fn competition_node_positions_update(
    ctx: &ReducerContext,
    positions: Vec<NodePositionUpdate>,
) -> Result<(), String> {
    for item in &positions {
        let competition_id = ctx.node_get_parent(item.node)?;
        ctx.auth_builder(competition_id)
            .permission(CompetitionPermissionsV1::COMPETITION_LAYOUT_EDIT)
            .authorize()?;
    }

    for update in positions {
        let Some(mut node) = ctx
            .db
            .tab_competition_node_position()
            .node_position()
            .filter(update.node.split())
            .next()
        else {
            return Err(format!("Couldnt find node {:?}", update.node));
        };

        node.position = update.position;

        ctx.db.tab_competition_node_position().id().update(node);
    }

    Ok(())
}

pub(super) trait NodePositionRead {}

pub(super) trait NodePositionWrite: NodePositionRead {
    fn node_position_insert(&self, node: NodeHandle) -> Result<(), String>;
    fn node_position_delete(&self, node: NodeHandle);
}

impl<Db: DbContext> NodePositionRead for Db {}

impl<Db: DbContext<DbView = Local>> NodePositionWrite for Db {
    fn node_position_insert(&self, node: NodeHandle) -> Result<(), String> {
        self.db()
            .tab_competition_node_position()
            .try_insert(TabCompetitionNodePosition::new(
                node,
                self.node_get_parent(node)?,
            ))?;
        Ok(())
    }

    fn node_position_delete(&self, node: NodeHandle) {
        self.db()
            .tab_competition_node_position()
            .node_position()
            .delete(node.split());
    }
}
