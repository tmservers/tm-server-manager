// The generator operates on a fixed set of players.
// It runs after the registration closes and uses Rules to determine matchups.
// The rules can be specified to capture previous events in a DAG.

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=generator))]
struct Generator {
    
}
