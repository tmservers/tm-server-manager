use spacetimedb::{
    AnonymousViewContext, Query, RawQuery, ReducerContext, Table, TimeDuration, Timestamp, Uuid,
    reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, node::NodeHandle},
    pipeline::{LbEntry, LbEntryKind},
    registration::{RegistrationStatus, tab_registration},
    user::{UserRead, UserV1, UserWrite},
};

#[table(
    accessor=tab_registeration_player, vis_private,
    index(accessor=user_registered, hash(columns=[user_id,registration_id]))
)]
#[derive(Debug, Clone, Copy)]
pub struct RegisterationPlayer {
    pub registered_at: Timestamp,
    #[index(hash)]
    pub registration_id: u32,
    pub user_id: u32,
}

#[view(accessor=registration_players, public)]
fn registration_players(ctx: &AnonymousViewContext, registration_id: u32) -> impl Query<RegisterationPlayer> {
    ctx.from
        .tab_registeration_player()
        .filter(|r| r.registration_id.eq(registration_id))
}

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
    fn registration_player_count(&self, registration_id: u32) -> usize;
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

        // Players registered at the same time still need an order which never changes.
        registered.sort_by_key(|f| (f.registered_at, f.user_id));

        registered
            .into_iter()
            .enumerate()
            .map(|(index, e)| {
                LbEntry::new(
                    e.user_id,
                    LbEntryKind::Standing,
                    (index + 1) as u16,
                    NodeHandle::RegistrationV1(registration_id),
                )
            })
            .collect()
    }

    fn registration_player_count(&self, registration_id: u32) -> usize {
        self.db_read_only()
            .tab_registeration_player()
            .registration_id()
            .filter(registration_id)
            .count()
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

    // Once it ended the nodes after it rely on the registered players.
    if registration.has_ended() {
        return Err("The registration already ended.".into());
    }

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
