use spacetimedb::{AnonymousViewContext, Identity, Query, view};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = tab_user))]
pub struct User {
    //ubisoft id of the user
    #[primary_key]
    pub account_id: String,

    name: String,

    //raw version of the club tag.
    club_tag: String,
    online: bool,
}

impl User {
    pub fn new(account_id: String, name: String) -> Self {
        User {
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
pub fn user(ctx: &AnonymousViewContext) -> Query<User> {
    ctx.from.tab_user().build()
}

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = user_identity))]
pub struct UserIdentity {
    #[primary_key]
    pub identity: Identity,
    //ubisoft if of the user
    pub account_id: String,
}

impl UserIdentity {
    pub fn new(account_id: String, identity: Identity) -> Self {
        Self {
            identity,
            account_id,
        }
    }
}
