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

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
struct ModeState {}
