//! Nodes which should have started once everything before them finished but could not, e.g. because no server was free.
//! They are kept with the reason, so the members of the competition see why, and tried again whenever a server becomes available.

use spacetimedb::{
    CtxDbRead, CtxDbWrite, CtxWithTimestamp, SpacetimeType, Table, Timestamp, ViewContext, table,
    view,
};

use crate::{
    authorization::Authorization,
    competition::node::{NodeHandle, NodeRead},
    schedule::{ScheduleRead, ScheduleWrite},
    tm_match::{MatchRead, MatchStatus, MatchWrite},
};

#[table(accessor = tab_node_start_failure, vis_private,
    index(accessor = node, hash(columns = [node_variant, node_id])))]
struct NodeStartFailure {
    #[auto_inc]
    #[primary_key]
    id: u64,
    #[index(hash)]
    competition_id: u32,
    node_variant: u8,
    node_id: u32,
    /// Why it could not start the last time it was tried.
    error: String,
    /// When it first could not start, the node which waits the longest is tried first.
    since: Timestamp,
}

/// A node of a competition which could not start, as its members see it.
#[derive(Debug, SpacetimeType, Clone, PartialEq, Eq)]
pub struct CompetitionNodeStartFailure {
    pub node: NodeHandle,
    pub error: String,
    pub since: Timestamp,
}

#[view(accessor = competition_node_start_failures, public)]
fn competition_node_start_failures(
    ctx: &ViewContext,
    competition_id: u32,
) -> Vec<CompetitionNodeStartFailure> {
    if ctx.auth_builder(competition_id).authorize().is_err() {
        return Vec::new();
    }
    ctx.node_start_failures(competition_id)
}

pub(crate) trait NodeStartRead {
    /// The nodes of the competition which could not start, the one which waits the longest first.
    fn node_start_failures(&self, competition_id: u32) -> Vec<CompetitionNodeStartFailure>;
}

impl<Db: CtxDbRead> NodeStartRead for Db {
    fn node_start_failures(&self, competition_id: u32) -> Vec<CompetitionNodeStartFailure> {
        let mut failures: Vec<NodeStartFailure> = self
            .db_read_only()
            .tab_node_start_failure()
            .competition_id()
            .filter(competition_id)
            .collect();
        failures.sort_by_key(|f| (f.since, f.id));
        failures
            .into_iter()
            .map(|f| CompetitionNodeStartFailure {
                node: NodeHandle::combine(f.node_variant, f.node_id),
                error: f.error,
                since: f.since,
            })
            .collect()
    }
}

pub(crate) trait NodeStartWrite {
    /// Starts a node whose inputs all finished, only matches and schedules start.
    /// If it cannot start the reason is kept, so it is tried again with [`NodeStartWrite::node_starts_retry`].
    fn node_start(&self, node: NodeHandle) -> Result<(), String>;
    /// Tries the nodes which could not start again, e.g. because a server became available.
    /// The one which waits the longest goes first, so it gets the server.
    fn node_starts_retry(&self);
}

impl<Db: CtxDbWrite + CtxWithTimestamp> NodeStartWrite for Db {
    fn node_start(&self, node: NodeHandle) -> Result<(), String> {
        // Starting again would set back a node which already runs, e.g. one prepared by hand meanwhile.
        if !awaits_start(self, node) {
            node_start_failure_clear(self, node);
            return Ok(());
        }

        let result = match node {
            NodeHandle::MatchV1(match_id) => self.match_set_preparation(match_id),
            NodeHandle::ScheduleV1(schedule_id) => {
                self.schedule_start_relative(schedule_id, self.timestamp())
            }
            _ => Ok(()),
        };

        match &result {
            Ok(()) => node_start_failure_clear(self, node),
            Err(error) => record(self, node, error),
        }
        result
    }

    fn node_starts_retry(&self) {
        let mut failures: Vec<NodeStartFailure> =
            self.db().tab_node_start_failure().iter().collect();
        failures.sort_by_key(|f| (f.since, f.id));

        for failure in failures {
            let node = NodeHandle::combine(failure.node_variant, failure.node_id);
            if let Err(error) = self.node_start(node) {
                log::info!("{node:?} still cannot start: {error}");
            }
        }
    }
}

/// Forgets that the node could not start, e.g. because it got deleted.
pub(crate) fn node_start_failure_clear(ctx: &impl CtxDbWrite, node: NodeHandle) {
    ctx.db()
        .tab_node_start_failure()
        .node()
        .delete(node.split());
}

/// Whether the node still waits to be started, so starting it now is what the graph expects.
fn awaits_start(ctx: &impl CtxDbRead, node: NodeHandle) -> bool {
    let waits = match node {
        NodeHandle::MatchV1(match_id) => ctx
            .match_find(match_id)
            .is_some_and(|m| m.status() == MatchStatus::Configured),
        NodeHandle::ScheduleV1(schedule_id) => ctx
            .schedule_find(schedule_id)
            .is_ok_and(|s| s.is_configured()),
        _ => false,
    };
    waits && ctx.node_pending_origins(node).is_empty()
}

fn record(ctx: &(impl CtxDbWrite + CtxWithTimestamp), node: NodeHandle, error: &str) {
    let failures = ctx.db().tab_node_start_failure();
    if let Some(mut failure) = failures.node().filter(node.split()).next() {
        failure.error = error.into();
        failures.id().update(failure);
        return;
    }

    let (node_variant, node_id) = node.split();
    failures.insert(NodeStartFailure {
        id: 0,
        competition_id: ctx.node_get_parent(node).unwrap_or(0),
        node_variant,
        node_id,
        error: error.into(),
        since: ctx.timestamp(),
    });
}

#[cfg(test)]
mod tests {
    use crate::{competition::node::NodeHandle, test_world::*, tm_match::MatchStatus};

    const NO_SERVER: &str = "no servers left";

    /// A configured match which waits for a leaderboard that is finished, so it should start right away.
    fn ready_match(world: &World, name: &str) -> (u32, NodeHandle) {
        let match_id = world.configured_match(name);
        let node = NodeHandle::MatchV1(match_id);
        let seeding = world.finished_leaderboard(&format!("Seeding of {name}"));
        world.wait_for(node, seeding);
        (match_id, node)
    }

    fn assert_waits_for_a_server(world: &World, match_id: u32) {
        assert_eq!(world.match_status(match_id), MatchStatus::Configured);
        let failures = world.start_failures();
        let failure = failures
            .iter()
            .find(|(node, _)| *node == NodeHandle::MatchV1(match_id))
            .expect("the match could not start");
        assert!(
            failure.1.contains(NO_SERVER),
            "unexpected reason: {}",
            failure.1
        );
    }

    #[test]
    fn a_match_which_could_not_get_a_server_starts_once_one_is_free() {
        let world = World::new();
        let server = world.server(1);
        let a = player(1);
        let first = world.live_match(&server, &[&a]);

        let (second, node) = ready_match(&world, "Second");
        assert_waits_for_a_server(&world, second);

        server.post(&world, [start_match()]);
        server.post(&world, round(1, &[(&a, 30_000, 10)]));
        server.post(&world, [end_match()]);

        assert_eq!(world.match_status(first), MatchStatus::Ended);
        assert_eq!(world.match_status(second), MatchStatus::Preparation);
        assert_eq!(world.server_of(node), Some(server.id));
        assert_eq!(world.start_failures(), vec![]);
    }

    #[test]
    fn a_match_which_could_not_get_a_server_starts_with_one_lent_and_verified_later() {
        let world = World::new();
        let (match_id, node) = ready_match(&world, "Final");
        assert_waits_for_a_server(&world, match_id);

        let server = world.logged_in_server(1);
        world.lend(&server);
        // A server nobody verified cannot be used yet.
        assert_waits_for_a_server(&world, match_id);

        world.verify(&server);

        assert_eq!(world.match_status(match_id), MatchStatus::Preparation);
        assert_eq!(world.server_of(node), Some(server.id));
        assert_eq!(world.start_failures(), vec![]);
    }

    #[test]
    fn a_match_which_could_not_get_a_server_starts_with_a_verified_one_lent_later() {
        let world = World::new();
        let server = world.logged_in_server(1);
        world.verify(&server);
        let (match_id, node) = ready_match(&world, "Final");
        assert_waits_for_a_server(&world, match_id);

        world.lend(&server);

        assert_eq!(world.match_status(match_id), MatchStatus::Preparation);
        assert_eq!(world.server_of(node), Some(server.id));
    }

    #[test]
    fn a_match_which_could_not_get_a_server_starts_once_one_comes_online() {
        let world = World::new();
        let server = world.server(1);
        server.disconnect(&world);
        let (match_id, node) = ready_match(&world, "Final");
        assert_waits_for_a_server(&world, match_id);

        server.reconnect(&world);

        assert_eq!(world.match_status(match_id), MatchStatus::Preparation);
        assert_eq!(world.server_of(node), Some(server.id));
    }

    #[test]
    fn a_match_which_could_not_get_a_server_starts_with_one_taken_from_another_match() {
        let world = World::new();
        let server = world.server(1);
        let holding = world.configured_match("Holding");
        world.as_owner(|ctx| crate::tm_match::match_assign_server(ctx, holding, server.id));
        let (match_id, node) = ready_match(&world, "Final");
        assert_waits_for_a_server(&world, match_id);

        world.as_owner(|ctx| crate::tm_match::match_server_revoke(ctx, holding));

        assert_eq!(world.match_status(match_id), MatchStatus::Preparation);
        assert_eq!(world.server_of(node), Some(server.id));
    }

    #[test]
    fn the_match_which_waits_the_longest_gets_the_free_server() {
        let world = World::new();
        let server = world.server(1);
        let a = player(1);
        world.live_match(&server, &[&a]);
        let (earlier, earlier_node) = ready_match(&world, "Earlier");
        world
            .test
            .clock
            .advance(spacetimedb::TimeDuration::from_micros(1));
        let (later, _) = ready_match(&world, "Later");

        server.post(&world, [start_match()]);
        server.post(&world, round(1, &[(&a, 30_000, 10)]));
        server.post(&world, [end_match()]);

        assert_eq!(world.match_status(earlier), MatchStatus::Preparation);
        assert_eq!(world.server_of(earlier_node), Some(server.id));
        assert_waits_for_a_server(&world, later);
    }

    #[test]
    fn a_deleted_match_does_not_wait_for_a_server_anymore() {
        let world = World::new();
        let (match_id, node) = ready_match(&world, "Final");
        assert_waits_for_a_server(&world, match_id);

        world.as_owner(|ctx| crate::competition::node::node_delete(ctx, node));

        assert_eq!(world.start_failures(), vec![]);
    }
}
