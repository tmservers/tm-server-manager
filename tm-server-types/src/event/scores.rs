use crate::{
    base::{Player, Team},
    event::Event,
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct Scores {
    #[cfg_attr(feature = "serde", serde(rename = "responseid"))]
    pub response_id: String,
    pub section: String,
    #[cfg_attr(feature = "serde", serde(rename = "useteams"))]
    pub use_teams: bool,

    #[cfg_attr(feature = "serde", serde(rename = "winnerteam"))]
    pub winner_team: i32,
    #[cfg_attr(feature = "serde", serde(rename = "winnerplayer"))]
    pub winner_player: String,

    pub teams: Vec<Team>,
    pub players: Vec<Player>,
}

impl<'a> From<&'a Event> for &'a Scores {
    fn from(value: &'a Event) -> Self {
        match value {
            Event::Scores(event) => event,
            _ => panic!("Wrong argument for this"),
        }
    }
}
