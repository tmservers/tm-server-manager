//! The migrated leaderboards and connections against what they output before the migration.
//!
//! The expected rows were taken by evaluating the same fixture with the old settings on main (ddb0690).
//! The fixture has no ties, because main ranked ties in the order of a `HashMap`.

use spacetimedb::{CtxDbRead, ReducerContext, Table, test_utils::TestAuth};
use tm_server_types::config::TmMode;

use super::*;
use crate::{
    competition::{
        connection::{
            ConnectionKind, ConnectionWrite, TabConnection, legacy_connection_data_count,
            legacy_connection_data_insert, migrate_connection_data_for_test,
        },
        node::NodeRead,
    },
    pipeline::test_util::test_context,
    tm_match::leaderboard::{MatchLeaderboardWrite, MatchRoundPlayer},
};

/// (match, user, round, points, time, position)
const ROUNDS: &[(u32, u32, u16, i32, i32, u16)] = &[
    (1, 1, 1, 100, 0, 1),
    (1, 2, 1, 60, 0, 2),
    (1, 3, 1, 40, 0, 3),
    (1, 4, 1, 30, 0, 4),
    (1, 2, 2, 100, 0, 1),
    (1, 3, 2, 60, 0, 2),
    (1, 1, 2, 40, 0, 3),
    (1, 4, 2, 30, 0, 4),
    (1, 3, 3, 100, 0, 1),
    (1, 4, 3, 60, 0, 2),
    (1, 1, 3, 40, 0, 3),
    (1, 2, 3, 30, 0, 4),
    (2, 4, 1, 100, 0, 1),
    (2, 1, 1, 70, 0, 2),
    (2, 2, 1, 40, 0, 3),
    (2, 3, 1, 30, 0, 4),
    (2, 4, 2, 100, 0, 1),
    (2, 2, 2, 80, 0, 2),
    (2, 3, 2, 50, 0, 3),
    (2, 1, 2, 20, 0, 4),
    // Time attack, player 3 did not finish the first round.
    (3, 1, 1, 0, 31000, 2),
    (3, 2, 1, 0, 30800, 1),
    (3, 3, 1, 0, 0, 0),
    (3, 1, 2, 0, 30500, 1),
    (3, 3, 2, 0, 31200, 2),
];

const ALL: u8 = 0;
const FIRST_N: u8 = 1;
const CUSTOM: u8 = 2;
const FIRST_OFFSET_N: u8 = 3;

/// The highest id of the old leaderboards, the ids after it are free.
const LAST_ID: u32 = 24;

/// Inserts an old leaderboard which gets the matches through old connection data.
fn legacy_leaderboard(
    ctx: &ReducerContext,
    id: u32,
    settings: Vec<LbSettingsV2>,
    inputs: &[(u32, u8, &[u8])],
) {
    ctx.db.tab_leaderboard_v2().insert(LeaderboardV2 {
        name: String::new(),
        settings,
        id,
        parent_id: 1,
        template: false,
        status: LeaderboardStatus::Configured,
    });
    for &(match_id, option, values) in inputs {
        let connection = ctx
            .connection_insert(TabConnection::for_test(
                NodeHandle::MatchV1(match_id),
                NodeHandle::LeaderboardV1(id),
                ConnectionKind::Data,
            ))
            .unwrap();
        legacy_connection_data_insert(ctx, connection.id, option, values.to_vec());
    }
}

fn sum() -> LbSettingsV2 {
    LbSettingsV2::Merge(LbMergeSettings::scores_for_test(false))
}

/// The database before the migration, with leaderboards created the way the old code created them.
fn before_the_migration() -> spacetimedb::test_utils::TestContext {
    let test = test_context(&[
        (1, TmMode::Rounds),
        (2, TmMode::Rounds),
        (3, TmMode::TimeAttack),
    ]);
    test.with_reducer_tx(TestAuth::internal(), |ctx| {
        for &(match_id, user_id, round, points, time, position) in ROUNDS {
            ctx.match_round_player_insert(MatchRoundPlayer::for_test_placed(
                match_id, user_id, round, points, time, position,
            ))?;
        }
        let all = |matches: &'static [u32]| -> Vec<(u32, u8, &'static [u8])> {
            matches.iter().map(|&m| (m, ALL, &[][..])).collect()
        };
        let best_rounds = LbFilterSettings::best_scores_for_test(false, 2);
        let best_match = LbFilterSettings::best_scores_for_test(true, 1);
        let average = LbSettingsV2::Merge(LbMergeSettings::scores_for_test(true));
        let plus_5 = LbSettingsV2::Remap(LbRemapSettings::add_to_score_for_test(5));

        legacy_leaderboard(ctx, 11, vec![LbSettingsV2::Finalize], &all(&[1]));
        legacy_leaderboard(ctx, 12, vec![LbSettingsV2::Finalize], &all(&[3]));
        legacy_leaderboard(ctx, 13, vec![sum()], &all(&[1, 2]));
        legacy_leaderboard(ctx, 14, vec![average], &all(&[1, 2]));
        legacy_leaderboard(
            ctx,
            15,
            vec![LbSettingsV2::Filter(best_rounds), sum()],
            &all(&[1, 2]),
        );
        legacy_leaderboard(
            ctx,
            16,
            vec![LbSettingsV2::Filter(best_match), sum()],
            &all(&[1, 2]),
        );
        legacy_leaderboard(ctx, 17, vec![plus_5, sum()], &all(&[1]));
        legacy_leaderboard(ctx, 21, vec![], &[(1, FIRST_N, &[2])]);
        legacy_leaderboard(ctx, 22, vec![], &[(1, CUSTOM, &[3, 1])]);
        legacy_leaderboard(ctx, 23, vec![], &[(1, FIRST_OFFSET_N, &[1, 2])]);
        legacy_leaderboard(
            ctx,
            LAST_ID,
            vec![LbSettingsV2::Finalize],
            &[(1, FIRST_N, &[3]), (2, FIRST_N, &[3])],
        );
        // The old code took the ids of the leaderboards from the shared counter.
        ctx.auto_inc_migration::<tab_leaderboard_v2__TableHandle>(LAST_ID);
        Ok::<_, String>(())
    })
    .unwrap();
    test
}

fn migrate(test: &spacetimedb::test_utils::TestContext) {
    test.with_reducer_tx(TestAuth::internal(), |ctx| {
        migrate_connection_data_for_test(ctx)?;
        migration_leaderboard_to_v3(ctx)
    })
    .unwrap();
}

/// (user, place, score, time) in the order the leaderboard outputs them.
fn output(test: &spacetimedb::test_utils::TestContext, id: u32) -> Vec<(u32, u16, i32, i32)> {
    test.anonymous_view_context()
        .node_resolve_output_data(NodeHandle::LeaderboardV1(id))
        .unwrap()
        .iter()
        .map(|e| (e.user_id, e.position, e.score, e.time))
        .collect()
}

#[test]
fn migrated_leaderboards_output_what_they_did_before() {
    let test = before_the_migration();
    migrate(&test);

    // The standings of a match.
    assert_eq!(
        output(&test, 11),
        vec![
            (3, 1, 200, 0),
            (2, 2, 190, 0),
            (1, 3, 180, 0),
            (4, 4, 120, 0)
        ]
    );
    // Summed and averaged over the rounds of two matches.
    assert_eq!(
        output(&test, 13),
        vec![
            (4, 1, 320, 0),
            (2, 2, 310, 0),
            (3, 3, 280, 0),
            (1, 4, 270, 0)
        ]
    );
    assert_eq!(
        output(&test, 14),
        vec![(4, 1, 64, 0), (2, 2, 62, 0), (3, 3, 56, 0), (1, 4, 54, 0)]
    );
    // The best two rounds of every user, summed.
    assert_eq!(
        output(&test, 15),
        vec![
            (4, 1, 200, 0),
            (2, 2, 180, 0),
            (1, 3, 170, 0),
            (3, 4, 160, 0)
        ]
    );
    // Every round is worth 5 more before they are summed.
    assert_eq!(
        output(&test, 17),
        vec![
            (3, 1, 215, 0),
            (2, 2, 205, 0),
            (1, 3, 195, 0),
            (4, 4, 135, 0)
        ]
    );
}

#[test]
fn the_best_match_of_every_user_is_what_it_was_before() {
    let test = before_the_migration();
    migrate(&test);

    // Users 3 and 4 tie, main put them in any order, so only the scores are compared.
    let mut scores: Vec<(u32, i32)> = output(&test, 16)
        .into_iter()
        .map(|(user, _, score, _)| (user, score))
        .collect();
    scores.sort();
    assert_eq!(scores, vec![(1, 180), (2, 190), (3, 200), (4, 200)]);
}

#[test]
fn migrated_connections_select_the_places_they_did_before() {
    let test = before_the_migration();
    migrate(&test);

    assert_eq!(output(&test, 21), vec![(3, 1, 200, 0), (2, 2, 190, 0)]);
    // Custom places come in the order they were picked.
    assert_eq!(output(&test, 22), vec![(1, 3, 180, 0), (3, 1, 200, 0)]);
    assert_eq!(output(&test, 23), vec![(2, 2, 190, 0), (1, 3, 180, 0)]);
}

#[test]
fn finalizing_now_ranks_every_match_with_its_own_mode() {
    let test = before_the_migration();
    migrate(&test);

    // Main ranked every round of a time attack on its own:
    // [(1, 1, 0, 30500), (2, 1, 0, 30800), (1, 2, 0, 31000), (3, 2, 0, 31200), (3, 0, 0, 0)]
    assert_eq!(
        output(&test, 12),
        vec![(1, 1, 0, 30500), (2, 2, 0, 30800), (3, 3, 0, 31200)]
    );
    // Main summed the standings of both matches as if they were rounds of the first one:
    // [(2, 1, 310, 0), (1, 2, 270, 0), (3, 3, 200, 0), (4, 4, 200, 0)]
    assert_eq!(
        output(&test, LAST_ID),
        vec![
            (3, 1, 200, 0),
            (2, 2, 190, 0),
            (1, 3, 180, 0),
            (4, 1, 200, 0),
            (2, 2, 120, 0),
            (1, 3, 90, 0)
        ]
    );
}

#[test]
fn migrating_again_changes_nothing_and_new_leaderboards_get_new_ids() {
    let test = before_the_migration();
    migrate(&test);
    let before = output(&test, 15);

    migrate(&test);

    assert_eq!(output(&test, 15), before);
    let cx = test.anonymous_view_context();
    assert_eq!(cx.db_read_only().tab_leaderboard_v2().count(), 0);
    assert_eq!(legacy_connection_data_count(&cx), 0);
    assert_eq!(cx.db_read_only().tab_leaderboard_v3().count(), 11);
    let created = test
        .with_reducer_tx(TestAuth::internal(), |ctx| {
            ctx.leaderboard_create(String::new(), 1, Vec2::ZERO, None, false)
        })
        .unwrap();
    assert_eq!(created.id, LAST_ID + 1);
}
