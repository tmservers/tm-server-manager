use spacetimedb::{SpacetimeType, Timestamp};

mod competition;
mod map;
mod r#match;

/// General purpose Record Type returned by query all sorts of leaderboards in the project.
/// All entries to a leaderboard should have a ghost associated with it.
#[derive(Debug, SpacetimeType)]
pub struct TmRecord {
    pub map_uid: String,
    pub player_uid: String,

    pub timestamp: Timestamp,

    pub time: u32,

    pub zone: String,
    pub player_name: String,

    //TODO: figure this out
    pub ghost: String,
}
