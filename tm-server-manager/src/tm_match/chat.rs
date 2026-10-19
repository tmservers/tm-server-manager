use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, node::NodeHandle},
    raw_server::{method::RawServerMethodWrite, occupation::TabRawServerOccupationRead},
    tm_match::{MatchStatus, tab_match},
    user::UserRead,
};

//TODO make private again
#[table(
    accessor= tab_match_chat,
    public
)]
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
        .unwrap();

    let user_name = ctx.user_name(user_id);

    let ingame_message = "[".to_string() + &user_name + "] " + &message;

    ctx.db.tab_match_chat().insert(MatchChat {
        message,
        user_id,
        match_id,
        id: 0,
        status: tm_match.status(),
    });

    ctx.send_raw_server_message(server_id, user_id, ctx.timestamp, ingame_message)?;

    Ok(())
}
