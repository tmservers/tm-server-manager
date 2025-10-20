#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RoundsLeaderboard {
    restarted: u16,
    current_round: u16,
    round: Vec<RoundsRound>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RoundsRound {
    players: Vec<RoundsRoundPlayer>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RoundsRoundPlayer {
    id: String,
    ghost: u64,
}
