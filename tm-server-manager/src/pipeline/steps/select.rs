use std::collections::HashSet;

use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    pipeline::{
        LbEntry, LbKey, LbScopeKey, LbStep,
        combinators::LbRows,
        context::LbContext,
        error::LbError,
        shape::{LbOrigins, LbShape, check_scope, check_standings},
    },
};

/// Selects entries by their place in the leaderboard, so it usually comes after a match or merge step.
/// The selected entries keep the order of their places, also entries with the same place keep theirs.
#[derive(Debug, SpacetimeType, Clone)]
pub(crate) struct LbSelectSettings {
    /// The places are selected in every group on its own, e.g. the first places of every match.
    pub(crate) scope: Vec<LbKey>,
    pub(crate) places: LbSelectPlaces,
}

#[derive(Debug, SpacetimeType, Clone)]
pub(crate) enum LbSelectPlaces {
    FirstN(u16),
    /// The places to take, starting at 1. Every place can only be taken once.
    Custom(Vec<u16>),
    FirstOffsetN(LbSelectFirstOffsetN),
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbSelectFirstOffsetN {
    pub(crate) offset: u16,
    pub(crate) take: u16,
}

impl LbStep for LbSelectSettings {
    fn apply(&self, _cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        Ok(entries
            .group_by(|e| LbScopeKey::of(&self.scope, e))
            .try_map(|_, entries| self.places.select(entries))?
            .flatten())
    }

    fn check(&self, _origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        check_scope(&self.scope, &input)?;
        if let LbSelectPlaces::Custom(places) = &self.places {
            let mut seen = HashSet::new();
            if let Some(place) = places.iter().find(|place| !seen.insert(**place)) {
                // A participant would come out twice, but a leaderboard has one standing per participant.
                return Err(LbError::new(format!(
                    "Place {place} is selected more than once."
                )));
            }
        }
        check_standings(&input)?;
        if input.origins == LbOrigins::Many && !self.scope.contains(&LbKey::Origin) {
            return Err(LbError::new(
                "Places only exist in a single leaderboard, merge the entries or select per origin.",
            ));
        }
        Ok(input)
    }
}

impl LbSelectPlaces {
    fn select(&self, leaderboard: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        // Places are only meaningful in a single leaderboard, several have to be merged or selected by origin.
        if let Some(first) = leaderboard.first() {
            let origin = first.get_node();
            if leaderboard.iter().any(|e| e.get_node() != origin) {
                return Err(LbError::new(
                    "Cannot select from entries of more than one origin, merge them or select per origin.",
                ));
            }
        }

        // The place of an entry is where its position puts it, a step before could have changed it.
        let leaderboard = leaderboard.sorted_by_key(|e| e.position);
        Ok(match self {
            LbSelectPlaces::FirstN(f) => leaderboard.into_iter().take(*f as usize).collect(),
            LbSelectPlaces::Custom(places) => leaderboard.places(places),
            LbSelectPlaces::FirstOffsetN(opt) => leaderboard
                .into_iter()
                .skip(opt.offset as usize)
                .take(opt.take as usize)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        competition::node::NodeHandle,
        pipeline::{
            LbEntryKind,
            test_util::{MATCH, standing, standing_in, test_context},
        },
    };

    fn standings() -> Vec<LbEntry> {
        (1..=5)
            .map(|place| standing(place as u32 * 10, place))
            .collect()
    }

    fn selected_users(places: LbSelectPlaces) -> Vec<u32> {
        let selected = places.select(standings()).unwrap();
        selected.iter().map(|e| e.user_id).collect()
    }

    #[test]
    fn selects_by_place() {
        assert_eq!(selected_users(LbSelectPlaces::FirstN(2)), vec![10, 20]);
        assert_eq!(
            selected_users(LbSelectPlaces::FirstOffsetN(LbSelectFirstOffsetN {
                offset: 1,
                take: 2
            })),
            vec![20, 30]
        );
    }

    #[test]
    fn custom_places_skip_places_which_do_not_exist() {
        assert_eq!(
            selected_users(LbSelectPlaces::Custom(vec![0, 3, 9, 2])),
            vec![30, 20]
        );
    }

    #[test]
    fn a_place_can_only_be_selected_once() {
        let select = |places| LbSelectSettings {
            scope: Vec::new(),
            places: LbSelectPlaces::Custom(places),
        };
        let standings = LbShape::standings(MATCH);

        assert!(select(vec![1, 3, 1]).check(MATCH, standings).is_err());
        assert!(select(vec![1, 3]).check(MATCH, standings).is_ok());
    }

    #[test]
    fn rejects_entries_of_more_than_one_origin() {
        let mut entries = standings();
        entries.push(LbEntry::new(
            60,
            LbEntryKind::Standing,
            1,
            NodeHandle::MatchV1(2),
        ));

        assert!(LbSelectPlaces::FirstN(2).select(entries).is_err());
    }

    #[test]
    fn an_origin_scope_selects_the_places_of_every_origin() {
        let other = NodeHandle::MatchV1(2);
        let entries = vec![
            standing(10, 1),
            standing(20, 2),
            standing_in(other, 30, 1),
            standing_in(other, 40, 2),
        ];

        let selected = LbSelectSettings {
            scope: vec![LbKey::Origin],
            places: LbSelectPlaces::FirstN(1),
        }
        .apply(
            &LbContext::new(&test_context(&[]).anonymous_view_context(), MATCH),
            entries,
        )
        .unwrap();

        let users: Vec<u32> = selected.iter().map(|e| e.user_id).collect();
        assert_eq!(users, vec![10, 30]);
    }

    #[test]
    fn places_come_from_the_positions_and_not_the_order_of_the_entries() {
        // E.g. after remapping the positions the entries are not in the order of their places anymore.
        let entries = vec![standing(30, 3), standing(10, 1), standing(20, 2)];

        let selected = LbSelectPlaces::FirstN(2).select(entries).unwrap();

        let users: Vec<u32> = selected.iter().map(|e| e.user_id).collect();
        assert_eq!(users, vec![10, 20]);
    }
}
