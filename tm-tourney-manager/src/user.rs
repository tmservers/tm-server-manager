use spacetimedb::{AnonymousViewContext, Identity, Query, Uuid, view};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tab_user))]
pub struct UserV1 {
    //ubisoft id of the user
    #[primary_key]
    pub account_id: Uuid,

    name: String,

    //raw version of the club tag.
    club_tag: String,
    online: bool,
}

impl UserV1 {
    pub fn new(account_id: Uuid, name: String) -> Self {
        UserV1 {
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

#[view(name=user,public)]
pub fn user(ctx: &AnonymousViewContext) -> Query<UserV1> {
    ctx.from.tab_user().build()
}

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = user_identity))]
pub struct UserIdentity {
    #[primary_key]
    pub identity: Identity,
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
