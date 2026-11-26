use spacetimedb::{AnonymousViewContext, Query, ReducerContext, Table, reducer, table, view};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, node::NodeHandle},
    raw_server::{method::RawServerMethodWrite, occupation::TabRawServerOccupationRead},
    tm_match::{MatchStatus, tab_match},
    user::UserRead,
};

#[table(accessor= tab_match_chat, vis_private)]
pub struct MatchChat {
    message: String,

    #[index(hash)]
    user_id: u32,
    #[index(hash)]
    match_id: u32,
    #[auto_inc]
    #[primary_key]
    id: u32,
    #[default(MatchStatus::Live)]
    status: MatchStatus,
}

impl MatchChat {
    pub(crate) fn new(match_id: u32, status: MatchStatus, user_id: u32, message: String) -> Self {
        Self {
            message,
            user_id,
            match_id,
            id: 0,
            status,
        }
    }
}

#[view(accessor = match_chat, public)]
fn match_chat(ctx: &AnonymousViewContext, match_id: u32) -> impl Query<MatchChat> {
    ctx.from.tab_match_chat().filter(|r| r.match_id.eq(match_id))
}

#[reducer]
fn unstable_match_chat_message(
    ctx: &ReducerContext,
    match_id: u32,
    message: String,
) -> Result<(), String> {
    let Some(tm_match) = ctx.db.tab_match().id().find(match_id) else {
        return Err("Match not found!".into());
    };

    let user_id = ctx
        .auth_builder(tm_match.parent_id)
        .permission(CompetitionPermissionsV1::MATCH_SEND_CHAT)
        .authorize()?;

    let server_id = ctx
        .occupation_with_occupier(NodeHandle::MatchV1(match_id))
        .ok_or("The match has no server to send the message to.")?;

    let user_name = ctx.user_name(user_id);

    let ingame_message = "[".to_string() + user_name.as_str() + "] " + message.as_str();

    ctx.db.tab_match_chat().insert(MatchChat {
        message,
        user_id,
        match_id,
        id: 0,
        status: tm_match.status(),
    });

    ctx.send_raw_server_message(server_id, user_id, ingame_message)?;

    Ok(())
}

pub(crate) trait MatchChatWrite {
    fn match_chat_insert(&self, message: MatchChat) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> MatchChatWrite for Db {
    fn match_chat_insert(&self, message: MatchChat) -> Result<(), String> {
        self.db().tab_match_chat().try_insert(message)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_world::*;

    #[test]
    fn the_chat_of_a_player_whose_connect_was_missed_is_kept() {
        let world = World::new();
        let server = world.server(1);
        let (a, b) = (player(1), player(2));
        let match_id = world.live_match(&server, &[&a]);

        server.post(&world, [chat(&b, "gg")]);

        let messages: Vec<(String, String)> = world
            .test
            .db
            .tab_match_chat()
            .match_id()
            .filter(match_id)
            .map(|chat| (world.account(chat.user_id), chat.message))
            .collect();
        assert_eq!(messages, vec![(b, "gg".to_string())]);
    }
}
