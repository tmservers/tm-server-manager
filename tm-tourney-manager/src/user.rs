use spacetimedb::{AnonymousViewContext, Identity, Query, Uuid, table, view};

#[table(accessor= tab_user)]
pub struct UserV1 {
    //ubisoft id of the user
    #[unique]
    pub account_id: Uuid,

    #[auto_inc]
    #[primary_key]
    pub internal_id: u32,

    name: String,

    //raw version of the club tag.
    club_tag: String,
    online: bool,
}

impl UserV1 {
    pub fn new(account_id: Uuid, name: String) -> Self {
        UserV1 {
            internal_id: 0,
            account_id,
            name,
            club_tag: "".into(),
            online: true,
        }
    }

    pub(crate) fn get_name(&self) -> &String {
        &self.name
    }
}

#[view(accessor=user,public)]
pub fn user(ctx: &AnonymousViewContext) -> impl Query<UserV1> {
    ctx.from.tab_user()
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
