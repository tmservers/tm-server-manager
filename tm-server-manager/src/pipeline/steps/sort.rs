use std::collections::HashSet;

use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    pipeline::{
        LbEntry, LbEntryKind, LbKey, LbParams, LbScopeKey, LbStep,
        combinators::LbRows,
        context::LbContext,
        error::LbError,
        shape::{LbOrigins, LbShape, check_participants, check_scope, check_standings},
    },
};

/// Ranks the standings of every input together, e.g. to seed the participants of several groups.
///
/// Entries with the same key keep their order, so the input with the lower slot ranks first.
/// Someone in several inputs keeps the best place, because a leaderboard has one standing per participant.
/// With [`LbRenumberSettings`] in front it interleaves the inputs: their first places, their second places and so on.
#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbSortSettings {
    param: LbParams,
}

impl LbStep for LbSortSettings {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        Ok(self.evaluate(cx.origin(), entries))
    }

    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        if input.is_empty() {
            return Ok(input);
        }
        check_participants(&input)?;
        check_standings(&input)?;
        Ok(LbShape {
            origins: LbOrigins::One(origin),
            ..input
        })
    }
}

impl LbSortSettings {
    fn evaluate(self, origin: NodeHandle, entries: Vec<LbEntry>) -> Vec<LbEntry> {
        let mut seen = HashSet::new();
        entries
            .sorted_by_key(|e| self.param.key(e))
            .into_iter()
            .filter(|e| seen.insert(e.participant()))
            // Ranked together they are a new leaderboard of the node.
            .map(|e| LbEntry {
                kind: LbEntryKind::Standing,
                ..e.with_origin(origin)
            })
            .collect::<Vec<_>>()
            .renumbered()
    }
}

/// Numbers the places of every group again from 1, which closes the gaps left by e.g. a select step.
/// Entries keep the order of their places, also entries with the same place keep theirs.
/// Only the places change, so the entries keep their origin.
#[derive(Debug, SpacetimeType, Clone)]
pub(crate) struct LbRenumberSettings {
    /// The places are numbered in every group on its own, e.g. per origin to interleave the inputs.
    scope: Vec<LbKey>,
}

impl LbStep for LbRenumberSettings {
    fn apply(&self, _cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        Ok(self.evaluate(entries))
    }

    fn check(&self, _origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        check_scope(&self.scope, &input)?;
        check_participants(&input)?;
        check_standings(&input)?;
        Ok(input)
    }
}

impl LbRenumberSettings {
    fn evaluate(&self, entries: Vec<LbEntry>) -> Vec<LbEntry> {
        entries
            .group_by(|e| LbScopeKey::of(&self.scope, e))
            .map(|_, entries| entries.sorted_by_key(|e| e.position).renumbered())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{
        LbSettingsV3, evaluate_steps,
        test_util::{standing_in, test_context},
    };

    const LEADERBOARD: NodeHandle = NodeHandle::LeaderboardV1(7);
    const FIRST: NodeHandle = NodeHandle::MatchV1(1);
    const SECOND: NodeHandle = NodeHandle::MatchV1(2);

    /// The first input has all its places, the second one only kept its 2nd and 5th place.
    fn inputs() -> Vec<LbEntry> {
        vec![
            standing_in(FIRST, 1, 1),
            standing_in(FIRST, 2, 2),
            standing_in(FIRST, 3, 3),
            standing_in(SECOND, 4, 2),
            standing_in(SECOND, 5, 5),
        ]
    }

    fn ranked(steps: &[LbSettingsV3], entries: Vec<LbEntry>) -> Vec<(u32, u16)> {
        let test = test_context(&[]);
        let ranked = evaluate_steps(
            &LbContext::new(&test.anonymous_view_context(), LEADERBOARD),
            steps,
            entries,
        )
        .unwrap();
        assert!(ranked.iter().all(|e| e.get_node() == LEADERBOARD));
        ranked.iter().map(|e| (e.user_id, e.position)).collect()
    }

    fn sort() -> LbSettingsV3 {
        LbSettingsV3::Sort(LbSortSettings {
            param: LbParams::Position,
        })
    }

    #[test]
    fn sorting_by_place_ranks_the_inputs_together_and_the_lower_slot_wins_ties() {
        assert_eq!(
            ranked(&[sort()], inputs()),
            vec![(1, 1), (2, 2), (4, 3), (3, 4), (5, 5)]
        );
    }

    #[test]
    fn renumbering_every_origin_first_interleaves_the_inputs() {
        let renumber = LbSettingsV3::Renumber(LbRenumberSettings {
            scope: vec![LbKey::Origin],
        });

        assert_eq!(
            ranked(&[renumber, sort()], inputs()),
            vec![(1, 1), (4, 2), (2, 3), (5, 4), (3, 5)]
        );
    }

    #[test]
    fn someone_in_several_inputs_keeps_the_best_place() {
        let entries = vec![
            standing_in(FIRST, 1, 1),
            standing_in(FIRST, 2, 2),
            standing_in(SECOND, 3, 1),
            standing_in(SECOND, 1, 2),
        ];

        assert_eq!(ranked(&[sort()], entries), vec![(1, 1), (3, 2), (2, 3)]);
    }

    #[test]
    fn rounds_have_no_places_to_rank_by() {
        let rounds = LbShape::rounds(FIRST);

        assert!(sort().check(LEADERBOARD, rounds).is_err());
        assert!(
            crate::pipeline::shape::check_steps(
                LEADERBOARD,
                &[LbSettingsV3::Match, sort()],
                rounds
            )
            .is_ok()
        );
    }

    #[test]
    fn renumbering_closes_the_gaps_in_the_order_of_the_places() {
        let renumbered = LbRenumberSettings { scope: Vec::new() }
            .evaluate(vec![standing_in(FIRST, 5, 5), standing_in(FIRST, 2, 2)]);

        let places: Vec<(u32, u16)> = renumbered.iter().map(|e| (e.user_id, e.position)).collect();
        assert_eq!(places, vec![(2, 1), (5, 2)]);
    }
}

#[cfg(test)]
impl LbSortSettings {
    pub(in crate::pipeline) fn by(param: LbParams) -> Self {
        LbSortSettings { param }
    }
}

#[cfg(test)]
impl LbRenumberSettings {
    /// Any renumbering, for property tests.
    pub(in crate::pipeline) fn strategy() -> impl proptest::strategy::Strategy<Value = Self> {
        use proptest::prelude::*;
        crate::pipeline::test_util::scope().prop_map(|scope| LbRenumberSettings { scope })
    }
}
