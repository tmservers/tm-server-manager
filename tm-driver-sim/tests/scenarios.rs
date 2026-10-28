//! Every scenario of `tests/scenarios` keeps every check, and its match is kept
//! as a snapshot. Add a file there to add a case.

use tm_driver_sim::{scenario, simulate_scenario};

#[test]
fn scenarios() {
    insta::glob!("scenarios/*.scn", |path| {
        let text = std::fs::read_to_string(path).unwrap();
        let world = scenario::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let outcome = simulate_scenario(&world);
        assert_eq!(outcome.failure, None, "\n{}", outcome.report);
        insta::assert_snapshot!(outcome.report);
    });
}
