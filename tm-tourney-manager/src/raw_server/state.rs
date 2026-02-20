use spacetimedb::{ReducerContext, Table, Uuid, reducer, table};

use crate::authorization::Authorization;

#[derive(Debug)]
#[table(name = tab_raw_server_player)]
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
    let server = ctx.get_server()?;

    log::error!("what");

    // Player is already present on the network.
    if let Some(mut player) = ctx.db.tab_raw_server_player().account_id().find(account_id) {
        if player.server_id == server.id {
            if (player.spectator && spectator) || (!player.spectator && !spectator) {
                log::error!("hmmge");
                return Err("Player was already in the state before the request.".into());
            }
            player.spectator = spectator;
            ctx.db.tab_raw_server_player().account_id().update(player);
            Ok(())
        } else {
            log::error!(
                "Server {} supposedly owned by {} attempted to modify a player which was on server {}. Sus",
                server.server_login,
                server.account_id,
                player.server_id
            );

            //TODO should we correct our mistake then because this should not be possible.
            //On the one hand wwe should trust us more because all servers could be sending malicious request displacing the player.
            //On the other hand every server can crash or disconnect failing to send the disconnection messages.
            //I guess we should trust the server but do more validation if it makes sense that the player is actually there or not.
            Err("Player was already connected to a server on the network.".into())
        }
    } else {
        //TODO check server side if its the server account id. We need to extract the server account id from the login token for that.

        ctx.db.tab_raw_server_player().try_insert(RawServerPlayer {
            server_id: server.id,
            account_id,
            spectator,
        })?;

        Ok(())
    }
}

#[reducer]
pub(super) fn raw_server_player_remove(
    ctx: &ReducerContext,
    account_id: Uuid,
) -> Result<(), String> {
    let server = ctx.get_server()?;

    if let Some(player) = ctx.db.tab_raw_server_player().account_id().find(account_id) {
        // Only the current server has permission to disconnect the player.
        if player.server_id == server.id {
            if !ctx.db.tab_raw_server_player().delete(player) {
                return Err("Could not delete player!".into());
            };
        } else {
            log::error!(
                "Server {} supposedly owned by {} attempted to remove a player which was on server {}. Sus",
                server.server_login,
                server.account_id,
                player.server_id
            );
            return Err(
                "Attempted to remove player from another server than he is currently on!".into(),
            );
        }
    } else {
        return Err("Player was not connected to a server.".into());
    }

    Ok(())
}
