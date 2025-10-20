use rounds::RoundsLeaderboard;

pub mod rounds;

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct Leaderboard {
    restarted: u16,
    warmup: u16,
    mode: ModeLeaderboard,
}

impl Leaderboard {
    pub fn new() -> Self {
        Self {
            restarted: 0,
            mode: todo!(),
            warmup: 0,
        }
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum ModeLeaderboard {
    Rounds(RoundsLeaderboard),
}

struct DesiredState {}
