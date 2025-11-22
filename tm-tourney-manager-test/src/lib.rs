#[cfg(test)]
use tm_tourney_manager_api_rs::*;

#[cfg(test)]
const PROJECT_NAME: &str = "tm-tourney-manager";

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

    db.exec();
}
