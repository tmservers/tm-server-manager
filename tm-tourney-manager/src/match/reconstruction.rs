#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct LiveMatch {
    restarted: u16,
    mode: ModeReconstruction,
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum ModeReconstruction {
    Rounds(RoundsReconstruction),
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct RoundsReconstruction {
    restarted: u16,
    current_round: u16,
}


struct DesiredState {
    
}