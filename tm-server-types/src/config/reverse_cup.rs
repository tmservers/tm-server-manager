use crate::config::{MapsPerMatch, RoundsPerMap, helper::FinishTimeout};

/// The script has the rounds script as a base so it is inheriting all the settings.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub struct ReverseCup {
    //Inherited Rounds settings.
    pub finish_timeout: FinishTimeout,
    pub maps_per_match: MapsPerMatch,
    pub use_custom_points_repartition: bool,
    pub points_repartition: Vec<u32>,
    pub rounds_per_map: RoundsPerMap,
    pub use_tie_breaker: bool,

    //New settings introduced
    /// The amount of points each player receives at the start
    pub starting_points: u32,
    /// When a player reach 0 points he is automatically eliminated
    pub disable_last_chance: bool,
    /// If whatever the issue of the round, all players will be in Last Chance,
    /// the round will be skipped to the next without playing it (all players will be in LastChance).
    pub allow_fast_forward_rounds: bool,
    /// Accelerate the distribution of points when the number of players alive decreases
    pub fast_forward_points_repartition: bool,
    /// Number of points players loose that give up a round
    pub dnf_points_loss: u32,
    pub last_chance_dnf_mode: LastChanceDnfMode,
    /// "Number of players awaited before starting the match (0 is automatic)"
    pub number_of_players: u32,
    //TODO pub complex_points_repartition as described in https://git.virtit.fr/beu/TM2020-Gamemodes/src/branch/master/TM_ReverseCup.Script.txt
}

impl ReverseCup {
    pub fn into_xml(&self) -> String {
        format!(
            r#"
        <setting name="S_RoundsPerMap" value="{}" type="integer"/>
        <setting name="S_MapsPerMatch" value="{}" type="integer"/>
        <setting name="S_PointsRepartition" value="{}" type="text"/>
        <setting name="S_UseCustomPointsRepartition" value="{}" type="boolean"/>
        <setting name="S_FinishTimeout" value="{}" type="integer"/>
        <setting name="S_UseTieBreak" value="{}" type="boolean"/>
        <setting name="S_PointsStartup" value="{}" type="integer"/>
        <setting name="S_DisableLastChance" value="{}" type="boolean"/>
        <setting name="S_AllowFastForwardRounds" value="{}" type="boolean"/>
        <setting name="S_FastForwardPointsRepartition" value="{}" type="boolean"/>
        <setting name="S_DNF_LossPoints" value="{}" type="integer"/>
        <setting name="S_LastChance_DNF_Mode" value="{}" type="integer"/>
        <setting name="S_NbOfPlayers" value="{}" type="integer"/>
        
            "#,
            Into::<i32>::into(self.rounds_per_map),
            Into::<i32>::into(self.maps_per_match),
            points_repartition_format(&self.points_repartition),
            self.use_custom_points_repartition,
            Into::<i32>::into(self.finish_timeout),
            self.use_tie_breaker,
            self.starting_points,
            self.disable_last_chance,
            self.allow_fast_forward_rounds,
            self.fast_forward_points_repartition,
            self.dnf_points_loss,
            Into::<i32>::into(self.last_chance_dnf_mode),
            self.number_of_players
        )
    }
}

fn points_repartition_format(points: &Vec<u32>) -> String {
    let mut string = String::new();
    for point in points {
        string += &point.to_string();
        string += ", "
    }
    string
}

impl Default for ReverseCup {
    fn default() -> Self {
        Self {
            finish_timeout: FinishTimeout::BasedOnMedal,
            maps_per_match: MapsPerMatch::One,
            points_repartition: vec![10, 6, 4, 3, 2, 1],
            rounds_per_map: RoundsPerMap::Unlimited,
            use_custom_points_repartition: false,
            use_tie_breaker: true,
            starting_points: 100,
            disable_last_chance: false,
            allow_fast_forward_rounds: true,
            fast_forward_points_repartition: true,
            dnf_points_loss: 20,
            last_chance_dnf_mode: LastChanceDnfMode::AllPlayers,
            number_of_players: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum LastChanceDnfMode {
    AllPlayers = 0,
    OnlyLeastCheckpoints = 1,
}

impl From<LastChanceDnfMode> for i32 {
    fn from(value: LastChanceDnfMode) -> Self {
        match value {
            LastChanceDnfMode::AllPlayers => 0,
            LastChanceDnfMode::OnlyLeastCheckpoints => 1,
        }
    }
}
