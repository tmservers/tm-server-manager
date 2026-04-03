use spacetimedb::{AnonymousViewContext, Identity, Query, Table, Uuid, ViewContext, table, view};

use crate::authorization::Authorization;

#[table(accessor= tab_user,vis_private)]
pub struct UserV1 {
    name: String,
    club_tag: String,
    zone: String,

    #[unique]
    account_id: Uuid,

    #[auto_inc]
    #[primary_key]
    id: u32,
}

impl UserV1 {
    pub fn new(account_id: Uuid) -> Self {
        UserV1 {
            id: 0,
            account_id,
            name: String::new(),
            club_tag: String::new(),
            zone: String::new(),
        }
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

#[view(accessor=my_user,public)]
pub fn my_user(ctx: &ViewContext) -> Option<UserV1> {
    let Ok(user) = ctx.user_id() else {
        return None;
    };
    ctx.db.tab_user().id().find(user)
}

#[view(accessor=users,public)]
pub fn users(ctx: &AnonymousViewContext) -> impl Query<UserV1> {
    ctx.from.tab_user()
}

#[table(accessor= tab_user_identity)]
struct UserIdentity {
    #[unique]
    identity: Identity,
    #[primary_key]
    user_id: u32,
}

impl UserIdentity {
    pub fn new(user_id: u32, identity: Identity) -> Self {
        Self { identity, user_id }
    }
}

#[table(accessor= tab_user_ids_map)]
struct UserIdsMap {
    #[primary_key]
    account_id: Uuid,
    #[unique]
    user_id: u32,
}

impl UserIdsMap {
    pub fn new(account_id: Uuid, internal_id: u32) -> Self {
        Self {
            user_id: internal_id,
            account_id,
        }
    }
}

pub(crate) trait UserRead {
    fn has_user(&self, account_id: Uuid) -> bool;
    fn get_user_id(&self, identity: Identity) -> Result<u32, String>;
    //fn user(&self, identity: Identity) -> Result<UserV1, String>;
    fn user_id_from_account(&self, account_id: Uuid) -> u32;
    fn user_account_from_id(&self, user_id: u32) -> Uuid;
}
impl<Db: spacetimedb::DbContext> UserRead for Db {
    fn get_user_id(&self, identity: Identity) -> Result<u32, String> {
        let Some(user) = self
            .db_read_only()
            .tab_user_identity()
            .identity()
            .find(identity)
        else {
            return Err("Identity not associated with a user account.".into());
        };

        Ok(user.user_id)
    }

    /* fn user(&self, identity: Identity) -> Result<UserV1, String> {
    let Some(user) = self
        .db_read_only()
        .tab_user_identity()
        .identity()
        .find(identity)
    else {
        return Err("Identity not associated with a user account.".into());
    };

    let Some(user) = self.db_read_only().tab_user().id().find(user.user_id) else {
        return Err("AccountId not associated with a user account.".into());
    };

    Ok(user) */

    fn user_id_from_account(&self, account_id: Uuid) -> u32 {
        self.db_read_only()
            .tab_user_ids_map()
            .account_id()
            .find(account_id)
            .unwrap()
            .user_id
    }

    fn has_user(&self, account_id: Uuid) -> bool {
        self.db_read_only()
            .tab_user_ids_map()
            .account_id()
            .find(account_id)
            .is_some()
    }

    fn user_account_from_id(&self, user_id: u32) -> Uuid {
        self.db_read_only()
            .tab_user_ids_map()
            .user_id()
            .find(user_id)
            .unwrap()
            .account_id
    }
}

pub(crate) trait UserWrite: UserRead {
    fn user_insert(&self, user: UserV1) -> Result<u32, String>;
    fn user_login(&self, user_id: u32, identity: Identity) -> Result<(), String>;
}
impl<Db: spacetimedb::DbContext<DbView = spacetimedb::Local>> UserWrite for Db {
    fn user_insert(&self, new_user: UserV1) -> Result<u32, String> {
        let user = self.db().tab_user().account_id().find(new_user.account_id);
        if let Some(mut user) = user {
            if user.name != new_user.name {
                user.name = new_user.name;
                let user = self.db().tab_user().id().update(user);
                return Ok(user.id);
            }
            Ok(user.id)
        } else {
            let user = self.db().tab_user().try_insert(new_user)?;
            self.db()
                .tab_user_ids_map()
                .try_insert(UserIdsMap::new(user.account_id, user.id))?;
            Ok(user.id)
        }
    }

    fn user_login(&self, user_id: u32, identity: Identity) -> Result<(), String> {
        if let Some(mut user_ident) = self.db().tab_user_identity().user_id().find(user_id) {
            if user_ident.identity == identity {
                return Ok(());
            } else {
                user_ident.identity = identity;

                self.db().tab_user_identity().user_id().update(user_ident);
            }
        } else {
            self.db()
                .tab_user_identity()
                .try_insert(UserIdentity::new(user_id, identity))?;
        }

        Ok(())
    }
}
