use spacetimedb::{ViewContext, view};

#[view(accessor= match_expected_players,public)]
pub fn match_expected_players(ctx: &ViewContext) -> Vec<()> {
    Vec::new()
}
