use spacetimedb::SpacetimeType;

use crate::leaderboard::{
    LbEntry, LbStep, combinators::LbRows, context::LbContext, error::LbError,
};

/// Selects entries by their place in the leaderboard, so it usually comes after a match or merge step.
#[derive(Debug, SpacetimeType, Clone)]
pub(crate) enum LbSelectSettings {
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
        self.evaluate(entries)
    }
}

impl LbSelectSettings {
    fn evaluate(&self, leaderboard: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        // Places are only meaningful in a single leaderboard, several have to be merged first.
        if let Some(first) = leaderboard.first() {
            let origin = first.get_node();
            if leaderboard.iter().any(|e| e.get_node() != origin) {
                return Err(LbError::new(
                    "Cannot select from entries of more than one origin, merge them first.",
                ));
            }
        }

        Ok(match self {
            LbSelectSettings::FirstN(f) => leaderboard.into_iter().take(*f as usize).collect(),
            LbSelectSettings::LastN(l) => leaderboard.into_iter().rev().take(*l as usize).collect(),
            LbSelectSettings::Custom(places) => leaderboard.places(places),
            LbSelectSettings::FirstOffsetN(opt) => leaderboard
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

    fn selected_users(select: LbSelectSettings) -> Vec<u32> {
        let selected = select.evaluate(standings()).unwrap();
        selected.iter().map(|e| e.user_id).collect()
    }

    #[test]
    fn selects_by_place() {
        assert_eq!(selected_users(LbSelectSettings::FirstN(2)), vec![10, 20]);
        // The last places come out in reverse.
        assert_eq!(selected_users(LbSelectSettings::LastN(2)), vec![50, 40]);
        assert_eq!(
            selected_users(LbSelectSettings::FirstOffsetN(LbSelectFirstOffsetN {
                offset: 1,
                take: 2
            })),
            vec![20, 30]
        );
    }

    #[test]
    fn custom_places_skip_places_which_do_not_exist() {
        assert_eq!(
            selected_users(LbSelectSettings::Custom(vec![0, 2, 9, 2])),
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

        assert!(LbSelectSettings::FirstN(2).evaluate(entries).is_err());
    }
}
