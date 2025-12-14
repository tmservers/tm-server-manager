use spacetimedb::{Identity, SpacetimeType, table};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = user))]
pub struct User {
    //ubisoft if of the user
    #[primary_key]
    pub id: String,

    name: String,

    //raw version of the club tag.
    club_tag: String,
    online: bool,
}

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = user_identity))]
pub struct UserIdentity {
    #[primary_key]
    pub identity: Identity,
    //ubisoft if of the user
    pub id: String,
}
