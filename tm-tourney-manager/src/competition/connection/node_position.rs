use spacetimedb::{reducer, view, ReducerContext, SpacetimeType, ViewContext};

use crate::{
    authorization::Authorization,
    competition::connection::{CompetitionConnection, NodeKindHandle},
};

#[spacetimedb::table(name = tab_competition_node_position,index(name=node_position,hash(columns=[node_variant,node_id])))]
#[derive(Debug, Clone, Copy)]
pub struct TabCompetitionNodePosition {
    // This is just so that we can update it.
    // Rework if multi col unique indices are there.
    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(btree)]
    competition_id: u32,

    //Maybe not necessary if we can expose another view with arg or something like that.
    //tournament_id: u32,
    node_id: u32,
    node_variant: u8,

    position: Vec2,
}

impl TabCompetitionNodePosition {
    pub(crate) fn new(node: NodeKindHandle, competition_id: u32) -> Self {
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
    node: NodeKindHandle,

    position: Vec2,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub struct NodePositionUpdate {
    node: NodeKindHandle,
    position: Vec2,
}

#[view(name=competition_node_position,public)]
pub fn competition_node_position(ctx: &ViewContext) -> Vec<CompetitionNodePosition> {
    ctx.db
        .tab_competition_node_position()
        .competition_id()
        //TODO actually make a view arg to filter not return everything.
        .filter(1u32..u32::MAX)
        .map(|v| CompetitionNodePosition {
            competition_id: v.competition_id,
            node: NodeKindHandle::combine(v.node_variant, v.node_id),
            position: v.position,
        })
        .collect()
}

#[reducer]
fn competition_node_position_update(
    ctx: &ReducerContext,
    node: NodeKindHandle,
    position: Vec2,
) -> Result<(), String> {
    let user = ctx.get_user()?;

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
    let user = ctx.get_user()?;

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
