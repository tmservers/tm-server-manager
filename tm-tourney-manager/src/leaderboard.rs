use spacetimedb::{SpacetimeType, table};

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct Leaderboard {
    players: Vec<String>,
}
