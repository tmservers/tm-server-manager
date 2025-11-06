#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum Scheduling {
    Manual,
    Independant,
    Inherited,
}
