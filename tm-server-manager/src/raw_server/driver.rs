//! The modes the bridge of a raw server plays with the mode driver.
//!
//! A bridge reports them after it logged in. An older bridge does not know newer
//! versions of the modes and never gets a match which plays one. A bridge which
//! reports nothing, e.g. one from before the mode driver, gets no driver matches.

use spacetimedb::{CtxDbRead, CtxDbWrite, ReducerContext, Table, reducer, table};
use tm_server_types::config::{BuiltInMode, DriverMode};

use crate::{authorization::Authorization, competition::connection::start::NodeStartWrite};

#[table(accessor = tab_raw_server_driver, vis_private)]
struct RawServerDriver {
    #[primary_key]
    server_id: u32,
    modes: Vec<BuiltInMode>,
}

/// The bridge of the server tells which modes it plays, by their names like `rounds-v1`.
/// Names this manager does not know, e.g. of a newer bridge, are left out.
#[reducer]
pub(crate) fn raw_server_driver_modes(
    ctx: &ReducerContext,
    modes: Vec<String>,
) -> Result<(), String> {
    let server_id = ctx.server_id()?;

    let mut known: Vec<BuiltInMode> = modes
        .iter()
        .filter_map(|name| BuiltInMode::named(name))
        .collect();
    known.sort();
    known.dedup();

    ctx.db.tab_raw_server_driver().server_id().delete(server_id);
    ctx.db.tab_raw_server_driver().try_insert(RawServerDriver {
        server_id,
        modes: known,
    })?;

    // Nodes which waited for a server that plays their mode may start now.
    ctx.node_starts_retry(server_id);

    Ok(())
}

pub(crate) trait RawServerDriverRead {
    /// Whether the bridge of the server plays the mode.
    fn raw_server_plays(&self, server_id: u32, mode: DriverMode) -> bool;
}

impl<Db: CtxDbRead> RawServerDriverRead for Db {
    fn raw_server_plays(&self, server_id: u32, mode: DriverMode) -> bool {
        let Some(driver) = self
            .db_read_only()
            .tab_raw_server_driver()
            .server_id()
            .find(server_id)
        else {
            return false;
        };
        match mode {
            DriverMode::BuiltIn(mode) => driver.modes.contains(&mode),
        }
    }
}

/// A bridge which logs in reports its modes again, it may be another version than before.
pub(crate) fn raw_server_driver_forget(ctx: &impl CtxDbWrite, server_id: u32) {
    ctx.db().tab_raw_server_driver().server_id().delete(server_id);
}

#[cfg(test)]
mod tests {
    use tm_server_types::config::{BuiltInMode, DriverMode};

    use super::RawServerDriverRead;
    use crate::{competition::node::NodeHandle, test_world::*, tm_match::MatchStatus};

    const ROUNDS: DriverMode = DriverMode::BuiltIn(BuiltInMode::RoundsV1);

    fn plays(world: &World, server: &Server, mode: BuiltInMode) -> bool {
        world
            .test
            .anonymous_view_context()
            .raw_server_plays(server.id, DriverMode::BuiltIn(mode))
    }

    /// A driver match which waits for a finished leaderboard, so it starts right away if it can.
    fn ready_driver_match(world: &World, mode: BuiltInMode) -> (u32, NodeHandle) {
        let match_id = world.configured_match_with("Final", driver_config(mode));
        let node = NodeHandle::MatchV1(match_id);
        world.wait_for(node, world.finished_leaderboard("Seeding"));
        (match_id, node)
    }

    #[test]
    fn a_bridge_plays_the_modes_it_reports_and_no_names_it_does_not_know() {
        let world = World::new();
        let server = world.server(1);
        assert!(!plays(&world, &server, BuiltInMode::RoundsV1));

        // A newer bridge knows a version this manager does not, a family is no version.
        server.report_modes(&world, &["rounds-v9", "knockout", "endurance-v1", "endurance-v1"]);

        assert!(plays(&world, &server, BuiltInMode::EnduranceV1));
        assert!(!plays(&world, &server, BuiltInMode::KnockoutV1));
        assert!(!plays(&world, &server, BuiltInMode::RoundsV1));

        // A report replaces the one before.
        server.report_modes(&world, &["rounds-v1"]);
        assert!(plays(&world, &server, BuiltInMode::RoundsV1));
        assert!(!plays(&world, &server, BuiltInMode::EnduranceV1));
    }

    #[test]
    fn a_driver_match_takes_a_server_whose_bridge_plays_its_mode() {
        let world = World::new();
        let old = world.server(1);
        old.report_modes(&world, &["knockout-v1"]);
        let new = world.server(2);
        new.report_modes(&world, &["knockout-v1", "rounds-v1"]);

        let (match_id, node) = ready_driver_match(&world, BuiltInMode::RoundsV1);

        assert_eq!(world.match_status(match_id), MatchStatus::Preparation);
        assert_eq!(world.server_of(node), Some(new.id));
        assert_eq!(world.start_failures(), vec![]);
    }

    #[test]
    fn a_driver_match_waits_until_a_bridge_plays_its_mode() {
        let world = World::new();
        let server = world.server(1);
        // A bridge from before the driver reports nothing.
        let (match_id, node) = ready_driver_match(&world, BuiltInMode::RoundsV1);

        assert_eq!(world.match_status(match_id), MatchStatus::Configured);
        let failures = world.start_failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, node);
        assert!(failures[0].1.contains("plays rounds-v1"), "{}", failures[0].1);

        // The bridge got updated.
        server.report_modes(&world, &["rounds-v1"]);

        assert_eq!(world.match_status(match_id), MatchStatus::Preparation);
        assert_eq!(world.server_of(node), Some(server.id));
        assert_eq!(world.start_failures(), vec![]);
    }

    #[test]
    fn a_server_assigned_by_hand_has_to_play_the_mode_too() {
        let world = World::new();
        let server = world.server(1);
        server.report_modes(&world, &["knockout-v1"]);
        let match_id = world.configured_match_with("Final", driver_config(BuiltInMode::RoundsV1));
        world.as_owner(|ctx| crate::tm_match::match_assign_server(ctx, match_id, server.id));

        let refused =
            world.try_as_owner(|ctx| crate::tm_match::match_set_preparation(ctx, match_id));

        assert!(refused.is_err_and(|e| e.contains("does not play rounds-v1")));
        assert_eq!(world.match_status(match_id), MatchStatus::Configured);
    }

    #[test]
    fn a_bridge_which_logs_in_again_has_to_report_its_modes_again() {
        let world = World::new();
        let server = world.server(1);
        server.report_modes(&world, &["rounds-v1"]);

        // Another version of the bridge took over the server.
        server.disconnect(&world);
        server.reconnect(&world);

        let cx = world.test.anonymous_view_context();
        assert!(!cx.raw_server_plays(server.id, ROUNDS));
    }

    /// Matches which use a mode keep getting servers for it when a bridge stops playing others.
    #[test]
    fn a_match_which_does_not_play_a_driver_mode_takes_any_server() {
        let world = World::new();
        let server = world.server(1);
        let match_id = world.configured_match("Final");
        world.as_owner(|ctx| crate::tm_match::match_set_preparation(ctx, match_id));

        assert_eq!(world.server_of(NodeHandle::MatchV1(match_id)), Some(server.id));
    }
}
