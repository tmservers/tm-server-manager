/// Makinig the server completly stateless and only a shell for physics calculation and managing the players.
#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct ServerState {
    players: Vec<String>,
    paused: bool,
}

impl Default for ServerState {
    fn default() -> Self {
        Self {
            players: Default::default(),
            paused: false,
        }
    }
}

/* #[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
struct ModeState {} */

// Time Attack rows only need match uid, time, player uid player name and ghost uuid
// can be sorted client side after all
// This would mean that its completly separate from the tournament system which is pretty nice.
// Also could snapsot it probably to build a custom leaderboard.
//Open question is how to do proper auth for polling clients.
