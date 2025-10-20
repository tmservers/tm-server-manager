#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RoundsLeaderboard {
    restarted: u16,
    current_round: u16,
}
