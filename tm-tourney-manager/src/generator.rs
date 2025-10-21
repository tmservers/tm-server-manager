// The generator operates on a fixed set of players.
// It runs after the registration closes and uses Rules to determine matchups.
// The rules can be specified to capture previous events in a DAG?.
// The generator has two modes: one for the matches that will happen
// and to get the full stage and match set immeditely.
// The second mode actually determines which players advance to the next stage based
// on a different rule set.

// Open questions:
// - What is the best way to separate/combine most of the logic between the 2 stages?
// - How to best separate the types in the containing construct and how to invoke in the children?
// - Can there be a general purpose visualizer (probably somewhat node based) to display your configured/generated event/match sets.

#[cfg_attr(feature = "spacetime", spacetimedb::table(name=generator))]
struct Generator {
    rules: bool,
    mode_rules: ModeRules,
}

impl Generator {
    pub fn execute(&self, context: &impl GeneratorContext) {}
}

pub trait GeneratorContext {
    fn gen_level();
}

#[derive(Debug)]
#[cfg_attr(feature = "spacetime", derive(spacetimedb::SpacetimeType))]
pub enum ModeRules {}
