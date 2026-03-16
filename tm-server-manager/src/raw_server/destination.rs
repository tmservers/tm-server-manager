use spacetimedb::{Query, ReducerContext, Uuid, ViewContext, table, view};

#[table(accessor=tab_player_destination)]
pub struct PlayerDestination {
    internal_account_id: u32,
    desination_server_id: u32,
}

#[view(accessor=my_player_destination,public)]
fn my_player_destination(ctx: &ViewContext) -> impl Query<PlayerDestination> {
    //ctx.
    todo!()
}
