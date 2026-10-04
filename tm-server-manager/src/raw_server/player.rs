use spacetimedb::{
    CtxDbWrite, ReducerContext, SpacetimeType, Table, Uuid, ViewContext, reducer, table, view,
};

use crate::{
    authorization::Authorization, competition::node::NodeRead,
    raw_server::occupation::TabRawServerOccupationRead,
};

//TODO make private again.
#[derive(Debug)]
#[table(accessor= tab_raw_server_player,public,vis_private)]
pub struct RawServerPlayer {
    #[primary_key]
    pub(crate) account_id: Uuid,

    #[index(hash)]
    pub(crate) server_id: u32,

    spectator: bool,
}

#[reducer]
pub(super) fn raw_server_player_add(
    ctx: &ReducerContext,
    account_id: Uuid,
    spectator: bool,
) -> Result<(), String> {
    let server_id = ctx.server_id()?;

    // Player is already present on the network.
    if let Some(mut player) = ctx.db.tab_raw_server_player().account_id().find(account_id) {
        if player.server_id == server_id {
            if (player.spectator && spectator) || (!player.spectator && !spectator) {
                log::info!("Player was already in state before the request");
                return Ok(());
            }
            player.spectator = spectator;
            ctx.db.tab_raw_server_player().account_id().update(player);
            Ok(())
        } else {
            player.spectator = spectator;
            player.server_id = server_id;
            log::warn!(
                "Player was already connected to {} but connected on {}. Updating but Susge",
                player.server_id,
                server_id
            );

            ctx.db.tab_raw_server_player().account_id().update(player);

            Ok(())
        }
    } else {
        ctx.db.tab_raw_server_player().try_insert(RawServerPlayer {
            server_id,
            account_id,
            spectator,
        })?;

        Ok(())
    }
}

pub(super) fn raw_server_player_remove(
    ctx: &ReducerContext,
    account_id: Uuid,
) -> Result<(), String> {
    let server_id = ctx.server_id()?;

    if let Some(player) = ctx.db.tab_raw_server_player().account_id().find(account_id) {
        // Only the current server has permission to disconnect the player.
        if player.server_id == server_id {
            if !ctx.db.tab_raw_server_player().delete(player) {
                return Err("Could not delete player!".into());
            };
        } else {
            return Err(
                "Attempted to remove player from another server than he is currently on!".into(),
            );
        }
    } else {
        return Err("Player was not connected to a server.".into());
    }

    Ok(())
}

pub(super) trait RawServerPlayerWrite {
    fn raw_server_player_server_remove(&self, server_id: u32) -> u64;
}

impl<Db: CtxDbWrite> RawServerPlayerWrite for Db {
    fn raw_server_player_server_remove(&self, server_id: u32) -> u64 {
        self.db()
            .tab_raw_server_player()
            .server_id()
            .delete(server_id)
    }
}

/* #[view(accessor= raw_server_current_players, public)]
fn raw_server_current_players(
    ctx: &AnonymousViewContext, /* TODO server_id */
) -> impl Query<RawServerPlayer> {
    let server_id = 1u32;
    ctx.from
        .tab_raw_server_player()
        .r#where(|p| p.server_id.eq(server_id))
} */

#[derive(Debug, SpacetimeType)]
pub struct PermittedPlayer {
    pub account_id: Uuid,
    mandatory: bool,
    pub only_spectator: bool,
}

impl PermittedPlayer {
    pub(crate) fn new(account_id: Uuid, mandatory: bool, only_spectator: bool) -> Self {
        Self {
            account_id,
            mandatory,
            only_spectator,
        }
    }
}

#[view(accessor= raw_server_permitted_players, public)]
fn raw_server_permitted_players(ctx: &ViewContext) -> Vec<PermittedPlayer> {
    let Ok(server_id) = ctx.server_id() else {
        return Vec::new();
    };

    let Some(node) = ctx.raw_server_occupation(server_id) else {
        return Vec::new();
    };

    ctx.node_permitted_players_input(node)
}
