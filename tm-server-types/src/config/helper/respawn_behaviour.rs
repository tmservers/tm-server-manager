#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum RespawnBehaviour {
    /// Use the default behavior of the gamemode
    Default = 0,
    /// Use the normal behavior like in TimeAttack.
    TimeAttack = 1,
    /// Do nothing.
    Ignore = 2,
    /// Give up before first checkpoint.
    GiveUpAtStart = 3,
    /// Always give up.
    GiveUpAlways = 4,
    /// Never give up.
    GiveUpNever = 5,
}

impl From<RespawnBehaviour> for i32 {
    fn from(value: RespawnBehaviour) -> Self {
        match value {
            RespawnBehaviour::Default => 0,
            RespawnBehaviour::TimeAttack => 1,
            RespawnBehaviour::Ignore => 2,
            RespawnBehaviour::GiveUpAtStart => 3,
            RespawnBehaviour::GiveUpAlways => 4,
            RespawnBehaviour::GiveUpNever => 5,
        }
    }
}
