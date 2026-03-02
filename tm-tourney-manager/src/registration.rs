use spacetimedb::{SpacetimeType, Timestamp, table};

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

#[table(accessor=tab_registration)]
pub struct Registration {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    parent_id: u32,
    project_id: u32,

    settings: RegistrationSettings,
}

impl Registration {
    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn get_project(&self) -> u32 {
        self.project_id
    }
}
