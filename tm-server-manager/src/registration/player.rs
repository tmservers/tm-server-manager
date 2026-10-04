use spacetimedb::{
    AnonymousViewContext, Query, RawQuery, ReducerContext, Table, TimeDuration, Timestamp, Uuid,
    reducer, table, view,
};
use tm_server_types::config::TmMode;

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, node::NodeHandle},
    leaderboard::LbEntry,
    registration::{RegistrationStatus, tab_registration},
    user::{UserRead, UserV1, UserWrite},
};

//TODO make this table private again.
#[table(
    accessor=tab_registeration_player,
    index(accessor=user_registered, hash(columns=[user_id,registration_id])),
    public
)]
#[derive(Debug, Clone, Copy)]
pub struct RegisterationPlayer {
    pub registered_at: Timestamp,
    #[index(hash)]
    pub registration_id: u32,
    pub user_id: u32,
}

/* #[view(accessor=temp_registration_player,public)]
fn temp_registration_player(
    ctx: &AnonymousViewContext, /* ,registration_id: u32 */
) -> Vec<RegisterationPlayer> {
    let registration_id = 2u32;
    ctx.registration_player(registration_id)
} */

/* #[view(accessor=unstable_registration_player,public)]
fn unstable_registration_player(ctx: &AnonymousViewContext) -> impl Query<RegisterationPlayer> {
    //ctx.from.tab_registeration_player()
    RawQuery::<RegisterationPlayer>::new(String::new())
} */

#[reducer]
fn register_player(ctx: &ReducerContext, registration_id: u32) -> Result<(), String> {
    let user_id = ctx.user_id()?;

    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Tried to register but the registration id does not exist.".into());
    };

    registration.player_registration_allowed(ctx)?;

    if ctx
        .db
        .tab_registeration_player()
        .user_registered()
        .filter((user_id, registration_id))
        .count()
        != 0
    {
        return Err("User is already registered for registration_id!".to_string());
    }

    ctx.db
        .tab_registeration_player()
        .try_insert(RegisterationPlayer {
            registration_id,
            user_id,
            registered_at: ctx.timestamp,
        })?;

    Ok(())
}

#[reducer]
fn unregister_player(ctx: &ReducerContext, registration_id: u32) -> Result<(), String> {
    let account_id = ctx.user_id()?;

    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Tried to register for a competition that doesnt exist.".into());
    };

    if registration.status != RegistrationStatus::Ongoing {
        return Err("Registration not active".into());
    }

    let Some(registred_user) = ctx
        .db
        .tab_registeration_player()
        .registration_id()
        .filter(registration_id)
        .find(|p| p.user_id == account_id)
    else {
        return Err("User is not registered for competition!".to_string());
    };

    if !ctx.db.tab_registeration_player().delete(registred_user) {
        return Err(format!(
            "Unexpected error occured deleting the user {} from {}",
            account_id, registration_id
        ));
    };

    Ok(())
}

pub(crate) trait RegistrationPlayerRead {
    //fn registration_player(&self, registration_id: u32) -> Vec<RegisterationPlayer>;
    fn registration_lb(&self, registration_id: u32) -> Vec<LbEntry>;
}
impl<Db: spacetimedb::CtxDbRead> RegistrationPlayerRead for Db {
    /* fn registration_player(&self, registration_id: u32) -> Vec<RegisterationPlayer> {
        let mut registered = self
            .db_read_only()
            .tab_registeration_player()
            .registration_id()
            .filter(registration_id)
            .collect::<Vec<_>>();

        registered.sort_by_key(|f| f.registered_at);

        registered
    } */
    fn registration_lb(&self, registration_id: u32) -> Vec<LbEntry> {
        let mut registered = self
            .db_read_only()
            .tab_registeration_player()
            .registration_id()
            .filter(registration_id)
            .collect::<Vec<_>>();

        registered.sort_by_key(|f| f.registered_at);

        registered
            .into_iter()
            .enumerate()
            .map(|(index, e)| {
                LbEntry::new(
                    e.user_id,
                    TmMode::Unknown,
                    (index + 1) as u16,
                    NodeHandle::RegistrationV1(registration_id),
                )
            })
            .collect()
    }
}

#[reducer]
fn unstable_manual_register_override_players(
    ctx: &ReducerContext,
    registration_id: u32,
    players: Vec<String>,
) -> Result<(), String> {
    let Some(registration) = ctx.db.tab_registration().id().find(registration_id) else {
        return Err("Tried to register but the registration id does not exist.".into());
    };

    ctx.auth_builder(registration.get_comp_id())
        //TODO
        .permission(CompetitionPermissionsV1::OWNER)
        .authorize()?;

    ctx.db
        .tab_registeration_player()
        .registration_id()
        .delete(registration_id);

    for (index, player) in players.iter().enumerate() {
        let account_id = Uuid::parse_str(player).unwrap();
        let user_id: u32;
        if ctx.has_user(account_id) {
            user_id = ctx.user_id_from_account(account_id);
        } else {
            let user = ctx.user_insert(UserV1::new(account_id))?;
            user_id = user;
        }

        ctx.db
            .tab_registeration_player()
            .try_insert(RegisterationPlayer {
                registration_id,
                user_id,
                registered_at: ctx
                    .timestamp
                    .checked_add(TimeDuration::from_micros(index as i64))
                    .unwrap(),
            })?;
    }
    Ok(())
}
