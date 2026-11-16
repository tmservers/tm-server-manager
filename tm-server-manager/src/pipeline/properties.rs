//! What every pipeline has to keep, checked with random entries and random steps.

use std::collections::{HashMap, HashSet};

use proptest::{prelude::*, sample::subsequence, strategy::ValueTree};
use spacetimedb::test_utils::TestContext;
use tm_server_types::config::TmMode;

use crate::{
    competition::node::NodeHandle,
    pipeline::{
        LbEntry, LbEntryKind, LbParticipant, LbScopeKey, LbSettingsV3,
        context::LbContext,
        error::LbError,
        evaluate_steps,
        shape::{LbOrigins, LbShape, check_steps},
        steps::{
            filter::LbFilterSettingsV2,
            merge::LbMergeSettingsV2,
            remap::LbRemapSettings,
            script::{LbScriptOutput, LbScriptSettings},
            select::{LbSelectFirstOffsetN, LbSelectPlaces, LbSelectSettings},
            sort::{LbRenumberSettings, LbSortSettings},
        },
        test_util::{for_team, params, round_in, scope, standing_in, test_context},
    },
};

/// The leaderboard the steps are evaluated for.
const LEADERBOARD: NodeHandle = NodeHandle::LeaderboardV1(9);
const REGISTRATION: NodeHandle = NodeHandle::RegistrationV1(1);
/// The matches rounds come from, every mode which ranks without a config.
const MATCHES: [(u32, TmMode); 3] = [
    (1, TmMode::Rounds),
    (2, TmMode::TimeAttack),
    (3, TmMode::Knockout),
];

thread_local! {
    /// The steps only read the modes of the matches, so every case of a thread shares the datastore.
    static TEST: TestContext = test_context(&MATCHES);
}

fn evaluate(steps: &[LbSettingsV3], entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
    TEST.with(|test| {
        let cx = LbContext::new(&test.anonymous_view_context(), LEADERBOARD);
        evaluate_steps(&cx, steps, entries)
    })
}

/// The rounds of the matches like a server reports them: every player at most once per round,
/// the places of a round by its points and the map changing every two rounds.
fn rounds_of(matches: impl Strategy<Value = usize>) -> impl Strategy<Value = Vec<LbEntry>> {
    let result = (matches, 1..=4u16, 1..=6u32, -5..=20i32, -1..=60_000i32);
    proptest::collection::vec(result, 0..24).prop_map(|results| {
        let mut seen = HashSet::new();
        let mut rounds: Vec<LbEntry> = results
            .into_iter()
            .filter(|&(m, round, user, _, _)| seen.insert((m, round, user)))
            .map(|(m, round, user, score, time)| {
                let entry = round_in(NodeHandle::MatchV1(MATCHES[m].0), user, round, score, time)
                    .set_map(u32::from(round.div_ceil(2)));
                // Some players play for a team, it is the same one in every round.
                for_team(entry, user % 3)
            })
            .collect();

        let mut places: HashMap<(NodeHandle, u16), Vec<usize>> = HashMap::new();
        for (index, entry) in rounds.iter().enumerate() {
            places
                .entry((entry.get_node(), entry.round))
                .or_default()
                .push(index);
        }
        for indices in places.values_mut() {
            indices.sort_by_key(|&i| (-rounds[i].score, rounds[i].user_id));
            for (place, &i) in (1u16..).zip(indices.iter()) {
                rounds[i].position = place;
            }
        }
        rounds
    })
}

/// The players of a registration in the order they registered.
fn registration() -> impl Strategy<Value = Vec<LbEntry>> {
    subsequence((1..=6u32).collect::<Vec<_>>(), 0..=6)
        .prop_shuffle()
        .prop_map(|users| {
            (1u16..)
                .zip(users)
                .map(|(place, user)| standing_in(REGISTRATION, user, place))
                .collect()
        })
}

/// Entries with the shape a node gets them in, from the matches or from the registration.
fn inputs() -> impl Strategy<Value = (Vec<LbEntry>, LbShape)> {
    prop_oneof![
        (
            rounds_of(0..MATCHES.len()),
            subsequence(vec![0, 1, 2], 1..=3)
        )
            .prop_map(|(rounds, connected)| {
                // A connected match gives rounds even before anything was played in it.
                let shape = connected
                    .iter()
                    .map(|&m| LbShape::rounds(NodeHandle::MatchV1(MATCHES[m].0)))
                    .reduce(LbShape::combine)
                    .expect("one match is connected at least");
                let connected: Vec<NodeHandle> = connected
                    .iter()
                    .map(|&m| NodeHandle::MatchV1(MATCHES[m].0))
                    .collect();
                let rounds = rounds
                    .into_iter()
                    .filter(|e| connected.contains(&e.get_node()))
                    .collect();
                (rounds, shape)
            }),
        registration().prop_map(|standings| (standings, LbShape::standings(REGISTRATION))),
    ]
}

fn select() -> impl Strategy<Value = LbSelectSettings> {
    let places = prop_oneof![
        (0..=6u16).prop_map(LbSelectPlaces::FirstN),
        subsequence((1..=6u16).collect::<Vec<_>>(), 0..=6)
            .prop_shuffle()
            .prop_map(LbSelectPlaces::Custom),
        (0..=3u16, 0..=3u16).prop_map(|(offset, take)| {
            LbSelectPlaces::FirstOffsetN(LbSelectFirstOffsetN { offset, take })
        }),
    ];
    (scope(), places).prop_map(|(scope, places)| LbSelectSettings { scope, places })
}

fn script() -> impl Strategy<Value = LbScriptSettings> {
    prop_oneof![
        Just(LbScriptSettings {
            source: "fn step(cx, entries) { entries }".into(),
            output: LbScriptOutput::Rounds,
        }),
        Just(LbScriptSettings {
            source: "fn step(cx, entries) { cx.match_standings(entries) }".into(),
            output: LbScriptOutput::Standings,
        }),
    ]
}

fn step() -> impl Strategy<Value = LbSettingsV3> {
    prop_oneof![
        Just(LbSettingsV3::Match),
        Just(LbSettingsV3::MatchTeams),
        LbMergeSettingsV2::strategy().prop_map(LbSettingsV3::Merge),
        LbMergeSettingsV2::strategy().prop_map(LbSettingsV3::MergeTeams),
        LbFilterSettingsV2::strategy().prop_map(LbSettingsV3::Filter),
        LbRemapSettings::strategy().prop_map(LbSettingsV3::Remap),
        select().prop_map(LbSettingsV3::Select),
        params().prop_map(|param| LbSettingsV3::Sort(LbSortSettings::by(param))),
        LbRenumberSettings::strategy().prop_map(LbSettingsV3::Renumber),
        script().prop_map(LbSettingsV3::Script),
    ]
}

/// Why the entries do not have the shape, the steps after them were checked against it.
fn mismatch(entries: &[LbEntry], shape: LbShape) -> Option<String> {
    if shape.is_empty() && !entries.is_empty() {
        return Some("entries came out although none were promised".into());
    }
    entries.iter().find_map(|e| {
        let wrong = match e.kind {
            LbEntryKind::Round if !shape.rounds => Some("a round"),
            LbEntryKind::Standing if !shape.standings => Some("a standing"),
            _ => None,
        };
        let wrong = wrong.or(match e.participant() {
            LbParticipant::Player(_) if !shape.players => Some("an entry of a player"),
            LbParticipant::Team(_) if !shape.teams => Some("an entry of a team"),
            _ => None,
        });
        let wrong = wrong.or((e.map_id != 0 && !shape.maps).then_some("an entry with a map"));
        let wrong = wrong.or(match shape.origins {
            LbOrigins::One(origin) if e.get_node() != origin => Some("an entry of another origin"),
            _ => None,
        });
        wrong.map(|wrong| format!("{wrong} came out of {shape:?}: {e:?}"))
    })
}

/// What tells entries apart, all of it.
fn fields(entries: &[LbEntry]) -> Vec<(u32, u32, u16, u16, u32, i32, i32, NodeHandle)> {
    entries
        .iter()
        .map(|e| {
            (
                e.user_id,
                e.team_id,
                e.position,
                e.round,
                e.map_id,
                e.score,
                e.time,
                e.get_node(),
            )
        })
        .collect()
}

/// The places are 1, 2, 3 and so on in the order of the entries and nobody comes twice.
fn assert_ranked(standings: &[LbEntry]) -> Result<(), TestCaseError> {
    let positions: Vec<u16> = standings.iter().map(|e| e.position).collect();
    prop_assert_eq!(positions, (1..=standings.len() as u16).collect::<Vec<_>>());
    let participants: HashSet<LbParticipant> = standings.iter().map(|e| e.participant()).collect();
    prop_assert_eq!(
        participants.len(),
        standings.len(),
        "a participant ranks twice: {:?}",
        standings
    );
    Ok(())
}

proptest! {
    #[test]
    fn what_the_steps_give_is_what_their_check_promised(
        (input, shape) in inputs(),
        steps in proptest::collection::vec(step(), 1..=4),
    ) {
        // Steps which their check rejects are never saved, and failing at runtime is allowed.
        let Ok(promised) = check_steps(LEADERBOARD, &steps, shape) else { return Ok(()) };
        let Ok(output) = evaluate(&steps, input) else { return Ok(()) };

        prop_assert_eq!(mismatch(&output, promised), None, "steps: {:?}", steps);
    }

    #[test]
    fn a_match_ranks_everyone_who_played_once(rounds in rounds_of(0..MATCHES.len())) {
        let players: HashSet<LbParticipant> = rounds.iter().map(|e| e.participant()).collect();

        let standings = evaluate(&[LbSettingsV3::Match], rounds).unwrap();

        // The matches come one after another, each ranked on its own.
        for (match_id, _) in MATCHES {
            let of_match: Vec<LbEntry> = standings
                .iter()
                .copied()
                .filter(|e| e.get_node() == NodeHandle::MatchV1(match_id))
                .collect();
            assert_ranked(&of_match)?;
        }
        let ranked: HashSet<LbParticipant> = standings.iter().map(|e| e.participant()).collect();
        prop_assert_eq!(ranked, players);
    }

    #[test]
    fn rounds_standings_are_the_points_of_all_rounds_whatever_order_they_come_in(
        (rounds, shuffled) in rounds_of(Just(0))
            .prop_flat_map(|rounds| (Just(rounds.clone()), Just(rounds).prop_shuffle())),
    ) {
        let mut points: HashMap<u32, i32> = HashMap::new();
        for round in &rounds {
            *points.entry(round.user_id).or_default() += round.score;
        }

        let standings = evaluate(&[LbSettingsV3::Match], rounds).unwrap();
        let again = evaluate(&[LbSettingsV3::Match], shuffled).unwrap();

        let scores: HashMap<u32, i32> = standings.iter().map(|e| (e.user_id, e.score)).collect();
        prop_assert_eq!(scores, points);
        let by_place: Vec<i32> = standings.iter().map(|e| e.score).collect();
        prop_assert!(by_place.is_sorted_by(|a, b| a >= b), "more points rank better: {:?}", by_place);
        // Only who of the players with the same points ranks first can depend on the order.
        let again_by_place: Vec<i32> = again.iter().map(|e| e.score).collect();
        prop_assert_eq!(by_place, again_by_place);
    }

    #[test]
    fn filtering_the_best_leaves_everyone_as_many_entries_as_the_others_in_their_group(
        rounds in rounds_of(0..MATCHES.len()),
        scope in scope(),
        param in params(),
        n in 1..=4u16,
    ) {
        let group_of = |e: &LbEntry| LbScopeKey::of(&scope, e);
        let mut counts: HashMap<(LbScopeKey, LbParticipant), usize> = HashMap::new();
        let mut best: HashMap<(LbScopeKey, LbParticipant), i64> = HashMap::new();
        for e in &rounds {
            let key = (group_of(e), e.participant());
            *counts.entry(key).or_default() += 1;
            let k = param.key(e);
            best.entry(key).and_modify(|b| *b = (*b).min(k)).or_insert(k);
        }
        let mut most: HashMap<LbScopeKey, usize> = HashMap::new();
        for (&(group, _), &count) in &counts {
            let m = most.entry(group).or_default();
            *m = (*m).max(count);
        }

        let filtered = evaluate(&[LbSettingsV3::Filter(LbFilterSettingsV2::best(scope.clone(), param, n))], rounds).unwrap();

        let mut kept: HashMap<(LbScopeKey, LbParticipant), Vec<i64>> = HashMap::new();
        for e in &filtered {
            kept.entry((group_of(e), e.participant())).or_default().push(param.key(e));
        }
        prop_assert_eq!(
            kept.keys().collect::<HashSet<_>>(),
            counts.keys().collect::<HashSet<_>>(),
            "everyone keeps entries in the groups they played in"
        );
        for (key, keys) in &kept {
            prop_assert_eq!(keys.len(), (n as usize).min(most[&key.0]), "count of {:?}", key);
            prop_assert_eq!(keys.iter().min(), Some(&best[key]), "the best entry of {:?} is kept", key);
        }
    }

    #[test]
    fn selecting_the_first_places_takes_them_as_they_are(
        rounds in rounds_of(Just(0)),
        first in 0..=6u16,
    ) {
        let standings = evaluate(&[LbSettingsV3::Match], rounds.clone()).unwrap();
        let select = LbSettingsV3::Select(LbSelectSettings {
            scope: Vec::new(),
            places: LbSelectPlaces::FirstN(first),
        });

        let selected = evaluate(&[LbSettingsV3::Match, select], rounds).unwrap();

        let taken = standings.len().min(first as usize);
        prop_assert_eq!(fields(&selected), fields(&standings[..taken]));
    }

    #[test]
    fn sorting_ranks_everyone_of_every_input_once_by_the_param(
        rounds in rounds_of(0..MATCHES.len()),
        param in params(),
    ) {
        let standings = evaluate(&[LbSettingsV3::Match], rounds.clone()).unwrap();
        let best: HashMap<LbParticipant, i64> = standings.iter().fold(HashMap::new(), |mut best, e| {
            let k = param.key(e);
            best.entry(e.participant()).and_modify(|b| *b = (*b).min(k)).or_insert(k);
            best
        });

        let sorted = evaluate(&[LbSettingsV3::Match, LbSettingsV3::Sort(LbSortSettings::by(param))], rounds).unwrap();

        assert_ranked(&sorted)?;
        prop_assert!(sorted.iter().all(|e| e.get_node() == LEADERBOARD));
        // Someone in several inputs keeps their best one.
        let keys: Vec<i64> = sorted.iter().map(|e| best[&e.participant()]).collect();
        prop_assert!(keys.is_sorted(), "sorted by {:?}: {:?}", param, keys);
        prop_assert_eq!(sorted.len(), best.len());
    }

    #[test]
    fn adding_and_subtracting_the_same_value_changes_nothing(
        rounds in rounds_of(0..MATCHES.len()),
        param in params(),
        value in 0..=1_000i32,
    ) {
        let standings = evaluate(&[LbSettingsV3::Match], rounds.clone()).unwrap();

        let shifted = evaluate(
            &[
                LbSettingsV3::Match,
                LbSettingsV3::Remap(LbRemapSettings::adding(param, value)),
                LbSettingsV3::Remap(LbRemapSettings::subtracting(param, value)),
            ],
            rounds,
        )
        .unwrap();

        prop_assert_eq!(fields(&shifted), fields(&standings));
    }
}

#[test]
fn the_random_steps_mostly_pass_their_check_and_run() {
    // Without this the property about checks could pass because nearly every case returned early.
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let cases = (inputs(), proptest::collection::vec(step(), 1..=4));
    let mut ran = 0;
    for _ in 0..500 {
        let ((input, shape), steps) = cases.new_tree(&mut runner).unwrap().current();
        if check_steps(LEADERBOARD, &steps, shape).is_ok() && evaluate(&steps, input).is_ok() {
            ran += 1;
        }
    }
    assert!(
        ran >= 150,
        "only {ran} of 500 random pipelines got checked and ran"
    );
}
