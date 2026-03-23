use std::collections::HashMap;

use spacetimedb::{AnonymousViewContext, Uuid};

use crate::{
    competition::{connection::tab_connection__view, node::NodeKindHandle},
    raw_server::player::PermittedPlayer,
};

pub fn match_permitted_players(ctx: &AnonymousViewContext, match_id: u32) -> Vec<PermittedPlayer> {
    let mut map: HashMap<Uuid, PermittedPlayer> = HashMap::new();
    let depending_nodes = ctx
        .db
        .tab_connection()
        .origins_of()
        .filter(NodeKindHandle::MatchV1(match_id).split())
        .filter(|c| c.is_data());

    for node in depending_nodes {
        let permitted_players = node
            .get_permitted_players(ctx)
            .into_iter()
            .map(|p| (p.account_id, p));
        map.extend(permitted_players);
    }

    let values = map.into_values().collect();

    log::warn!("{:?}", values);

    values
}
