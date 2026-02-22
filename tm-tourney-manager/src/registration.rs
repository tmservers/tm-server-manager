use spacetimedb::{SpacetimeType, Timestamp};

mod player;
mod team;

#[derive(Debug, SpacetimeType)]
pub enum RegistrationSettings {
    Players(RegistrationPlayerSettings),
    Team(RegistrationTeamSettings),
    None,
}

#[derive(Debug, SpacetimeType)]
pub struct RegistrationPlayerSettings {
    player_limit: Option<u32>,
    registration_deadline: Timestamp,
}

#[derive(Debug, SpacetimeType)]
pub struct RegistrationTeamSettings {
    team_limit: Option<u32>,
    team_size_min: u8,
    team_size_max: u8,
    registration_deadline: Timestamp,
}

//TODO make this a table somehow.
#[derive(Debug, SpacetimeType)]
pub struct TeamInfo {
    registered_at: Timestamp,
    name: String,
    creator: String,
    members: Vec<String>,
}
