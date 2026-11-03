use spacetimedb::SpacetimeType;

use crate::leaderboard::LbEntry;

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

impl LbSelectSettings {
    pub(super) fn evaluate(&self, leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        // Places are only meaningful in a single leaderboard, several have to be merged first.
        if let Some(first) = leaderboard.first() {
            let origin = first.get_node();
            if leaderboard.iter().any(|e| e.get_node() != origin) {
                log::error!(
                    "Cannot select from entries of more than one origin, merge them first."
                );
                return Vec::new();
            }
        }

        match self {
            LbSelectSettings::FirstN(f) => leaderboard.into_iter().take(*f as usize).collect(),
            LbSelectSettings::LastN(l) => leaderboard.into_iter().rev().take(*l as usize).collect(),
            LbSelectSettings::Custom(places) => places
                .iter()
                .filter_map(|place| {
                    let index = place.checked_sub(1)?;
                    leaderboard.get(index as usize).copied()
                })
                .collect(),
            LbSelectSettings::FirstOffsetN(opt) => leaderboard
                .into_iter()
                .skip(opt.offset as usize)
                .take(opt.take as usize)
                .collect(),
        }
    }
}
