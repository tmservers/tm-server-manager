#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum Scheduling {
    Manual,
    Independant,
    // The inherited option should register a scheduled reducer.
    // In this way it could have match groups and dependencies that it triggers multiple matches.
    // This field should be the id of the scheduled reducer.
    Inherited(u64),
}

#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub struct ScheduleOptions {}
