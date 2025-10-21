use spacetimedb::{Identity, SpacetimeType, table};

#[cfg_attr(feature = "spacetime", spacetimedb::table(name = user, public))]
pub struct User {
    #[unique]
    identity: Identity,
    //ubisoft if of the user
    #[primary_key]
    pub id: String,

    name: String,

    //raw version of the club tag.
    club_tag: String,
    online: bool,
}

/* #[derive(Debug, Clone, Copy, SpacetimeType)]
pub enum Roles {
    User,
    TrackmaniaServer,
} */
