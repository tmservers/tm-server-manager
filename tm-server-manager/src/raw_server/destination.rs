use spacetimedb::{DbContext, Local, SpacetimeType, Table, Uuid, ViewContext, table, view};

use crate::{
    authorization::Authorization,
    competition::node::{NodeHandle, NodeRead},
    raw_server::{occupation::TabRawServerOccupationRead, tab_raw_server__view},
    tm_match::tab_match__view,
    user::tab_user_ids_map__view,
};

#[table(accessor=tab_player_destination,
    index(accessor=competition_player, hash(columns=[competition_id,user_id]))
)]
pub struct TabPlayerDestination {
    #[index(hash)]
    pub competition_id: u32,

    #[index(hash)]
    pub match_id: u32,

    // Destination for the player
    #[index(hash)]
    pub destination_server_id: u32,
    // So this would need to be 0 to move all players?
    pub user_id: u32,
}

#[derive(Debug, SpacetimeType)]
pub struct PlayerDestination {
    account_id: Uuid,
    server_account_id: Uuid,
}

#[view(accessor=raw_server_player_destination,public)]
fn raw_server_player_destination(ctx: &ViewContext) -> Vec<PlayerDestination> {
    let Ok(this_server) = ctx.get_server() else {
        return Vec::new();
    };

    let Some(node) = ctx.raw_server_occupation(this_server.id) else {
        return Vec::new();
    };

    let Ok(competition_id) = ctx.node_get_parent(node) else {
        return Vec::new();
    };

    ctx.db
        .tab_player_destination()
        .competition_id()
        .filter(competition_id)
        .filter(|p| p.destination_server_id != this_server.id)
        .map(|r| {
            if r.user_id == 0 {
                PlayerDestination {
                    account_id: Uuid::MAX,
                    server_account_id: ctx
                        .db
                        .tab_raw_server()
                        .id()
                        .find(r.destination_server_id)
                        .unwrap()
                        .server_account_id,
                }
            } else {
                PlayerDestination {
                    account_id: ctx
                        .db
                        .tab_user_ids_map()
                        .user_id()
                        .find(r.user_id)
                        .unwrap()
                        .account_id,
                    server_account_id: ctx
                        .db
                        .tab_raw_server()
                        .id()
                        .find(r.destination_server_id)
                        .unwrap()
                        .server_account_id,
                }
            }
        })
        .collect()
}

pub(crate) trait TabRawServerDestinationRead {}
impl<Db: DbContext> TabRawServerDestinationRead for Db {}

pub(crate) trait TabRawServerDestinationWrite: TabRawServerDestinationRead {
    fn destination_claim(&self, node: NodeHandle) -> Result<(), String>;
}
impl<Db: DbContext<DbView = Local>> TabRawServerDestinationWrite for Db {
    fn destination_claim(&self, node: NodeHandle) -> Result<(), String> {
        let players = self.node_permitted_players_input(node);
        for player in players {
            /* self.db()
            .tab_player_destination()
            .try_insert(TabPlayerDestination {
                match_id,
                competition_id,
                destination_server_id: server_id,
                //PERF: This is a back and forth with other views i think and could be done cleaner.
                //no time for now tho. This would require an overhaul in many places includinig leaderboards and stuff.
                user_id: self
                    .db_read_only()
                    .tab_user_ids_map()
                    .account_id()
                    .find(player.account_id)
                    .unwrap()
                    .user_id,
            })?; */
        }

        Ok(())
    }
}

// who can claim destination????
// Manual or Automatic -> both cases
// -> Match who just got switched into prepare. -> Claim required players for destination
//  -> So server occupied with match in comp should be allowed to claim.
// -> User with claim players permission ???

// What happens with an open match?
// Should we allow to claim all? -> Definetly no that would be bad......
// Probably move the discovery server into club? -> No

// How to clean this whole thing up?
// -> Server (Match) is responsible to drop the claim again when the claim reason (match) is over.
// -> If user sets up a claim how does it clean itself up?
// -> Manual cleanup is error prone :(
// -> You would have to set it up in advance....

// So the discovery match which is open claims players for itself.
// -> There can be only one all player claim open for a given competition.... :)
// -> It only captures the negative of other active claims ???? -> how to query that?

// Only a match can do a claim.
// And it is done through a checkbox e.g. claim_players (defualt true) which automatically generates the claim
// IF the match is open we move everybody there ????
// IF match is done it drops the claim :)
