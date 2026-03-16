/* #[cfg(test)]
use tm_tourney_manager_api_rs::*;

#[cfg(test)]
const PROJECT_NAME: &str = "tm-server-manager";

#[cfg(test)]
mod setup;

#[test]
#[should_panic]
fn enforce_unique_tournament_name() {
    let db = setup::test();

    db.reducers.on_create_tournament(|ctx, _| {
        if let spacetimedb_sdk::Status::Failed(event) = &ctx.event.status {
            panic!("{event}")
        }
    });

    _ = db
        .reducers
        .create_tournament("there shouldn't be duplicate names".into());

    _ = db
        .reducers
        .create_tournament("there shouldn't be duplicate names".into());

    db.wait_for_msgs();
}

#[test]
fn normal_account_cant_post_event() {
    use spacetimedb_sdk::{DbContext, Table};

    let db = setup::test();

    /* db.reducers.on_post_event(|ctx, _| {
        if let spacetimedb_sdk::Status::Failed(event) = &ctx.event.status {
            panic!("{event}")
        }
    }); */

    _ = db.reducers.create_tournament("mytournament".into());

    _ = db.reducers.post_event(Event::WarmupEnd);

    db.subscription_builder().subscribe_to_all_tables();

    db.wait_for_msgs();

    insta::assert_debug_snapshot!(db.db.tournament().iter().collect::<Vec<_>>());
}
 */
