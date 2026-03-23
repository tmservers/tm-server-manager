use spacetimedb::{Identity, Uuid, ViewContext, table, view};

use crate::authorization::Authorization;

#[table(accessor= tab_user)]
pub struct UserV1 {
    name: String,
    club_tag: String,
    zone: String,

    #[unique]
    pub account_id: Uuid,

    #[auto_inc]
    #[primary_key]
    pub internal_id: u32,
}

impl UserV1 {
    pub fn new(account_id: Uuid) -> Self {
        UserV1 {
            internal_id: 0,
            account_id,
            name: String::new(),
            club_tag: String::new(),
            zone: String::new(),
        }
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub(crate) fn get_name(&self) -> &String {
        &self.name
    }
}

#[view(accessor=my_user,public)]
pub fn my_user(ctx: &ViewContext) -> Option<UserV1> {
    let Ok(user) = ctx.get_user_account() else {
        return None;
    };
    ctx.db.tab_user().account_id().find(user)
}

#[table(accessor= tab_user_identity)]
pub struct UserIdentity {
    #[unique]
    pub identity: Identity,
    #[primary_key]
    pub account_id: Uuid,
}

impl UserIdentity {
    pub fn new(account_id: Uuid, identity: Identity) -> Self {
        Self {
            identity,
            account_id,
        }
    }
}

#[table(accessor= tab_user_ids_map)]
pub struct UserIdsMap {
    #[primary_key]
    pub account_id: Uuid,
    #[unique]
    pub user_id: u32,
}

impl UserIdsMap {
    pub fn new(account_id: Uuid, internal_id: u32) -> Self {
        Self {
            user_id: internal_id,
            account_id,
        }
    }
}
