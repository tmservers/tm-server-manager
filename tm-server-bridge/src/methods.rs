use tm_tourney_manager_api_rs::{EventContext, TmServerMethod};

pub fn method_call_received(ctx: &EventContext, new: &TmServerMethod) {
    println!("{new:?}");
}
