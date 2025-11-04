use rounds::RoundsLeaderboard;

pub mod rounds;

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum MatchLeaderboardRules {
    Rounds(RoundsLeaderboard),
}
impl MatchLeaderboardRules {
    pub fn new() -> Self {
        Self::Rounds(RoundsLeaderboard::default())
    }
}

struct DesiredState {}
