#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum Registration {
    Players(PlayerRegistration),
    Team(TeamRegistration),
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct PlayerRegistration {
    player_limit: Option<u32>,
    players: Vec<String>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct TeamRegistration {
    team_limit: Option<u32>,
    team_size_min: u8,
    team_size_max: u8,
    teams: Vec<TeamInfo>,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct TeamInfo {
    registered_at: spacetimedb::Timestamp,
    name: String,
    members: Vec<String>,
}
