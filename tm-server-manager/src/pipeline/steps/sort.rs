use std::collections::HashSet;

use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    pipeline::{
        LbEntry, LbEntryKind, LbKey, LbParams, LbScopeKey, LbStep,
        combinators::LbRows,
        context::LbContext,
        error::LbError,
        shape::{LbOrigins, LbShape, check_scope},
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

/// Numbers the places of every group again in the order of its entries, starting at 1.
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
        check_standings(&input)?;
        Ok(input)
    }
}

impl LbRenumberSettings {
    fn evaluate(&self, entries: Vec<LbEntry>) -> Vec<LbEntry> {
        entries
            .group_by(|e| LbScopeKey::of(&self.scope, e))
            .map(|_, entries| entries.renumbered())
            .flatten()
    }
}

/// Places only exist in standings, the place of a round is its own.
fn check_standings(input: &LbShape) -> Result<(), LbError> {
    if input.rounds {
        return Err(LbError::new(
            "Places only exist in standings, add a match or merge step first.",
        ));
    }
    Ok(())
}
