use spacetimedb::SpacetimeType;

use crate::leaderboard::{
    LbEntry, LbKey, LbScopeKey, LbStep, combinators::LbRows, context::LbContext, error::LbError,
};

/// Selects entries by their place in the leaderboard, so it usually comes after a match or merge step.
#[derive(Debug, SpacetimeType, Clone)]
pub(crate) struct LbSelectSettings {
    /// The places are selected in every group on its own, e.g. the first places of every match.
    pub(crate) scope: Vec<LbKey>,
    pub(crate) places: LbSelectPlaces,
}

#[derive(Debug, SpacetimeType, Clone)]
pub(crate) enum LbSelectPlaces {
    FirstN(u8),
    LastN(u8),
    /// The places to take, starting at 1.
    Custom(Vec<u8>),
    FirstOffsetN(LbSelectFirstOffsetN),
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbSelectFirstOffsetN {
    pub(crate) offset: u8,
    pub(crate) take: u8,
}

impl LbStep for LbSelectSettings {
    fn apply(&self, _cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        Ok(entries
            .group_by(|e| LbScopeKey::of(&self.scope, e))
            .try_map(|_, entries| self.places.select(entries))?
            .flatten())
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

        Ok(match self {
            LbSelectPlaces::FirstN(f) => leaderboard.into_iter().take(*f as usize).collect(),
            LbSelectPlaces::LastN(l) => leaderboard.into_iter().rev().take(*l as usize).collect(),
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
        leaderboard::{LbEntryKind, test_util::standing},
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
        // The last places come out in reverse.
        assert_eq!(selected_users(LbSelectPlaces::LastN(2)), vec![50, 40]);
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
            selected_users(LbSelectPlaces::Custom(vec![0, 2, 9, 2])),
            vec![20, 20]
        );
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
}
