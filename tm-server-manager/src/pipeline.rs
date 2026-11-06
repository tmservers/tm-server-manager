//! The pipeline of steps which turns the entries of nodes into leaderboards.
//! Leaderboard nodes and data connections both run their steps with it.

use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    pipeline::{
        context::LbContext,
        error::LbError,
        modes::match_standings,
        shape::LbShape,
        steps::{
            filter::LbFilterSettingsV2,
            merge::LbMergeSettingsV2,
            remap::LbRemapSettings,
            select::LbSelectSettings,
            sort::{LbRenumberSettings, LbSortSettings},
        },
    },
};

mod combinators;
pub(crate) mod context;
pub(crate) mod error;
pub(crate) mod modes;
pub(crate) mod shape;
pub(crate) mod steps;
#[cfg(test)]
pub(crate) mod test_util;

/// A step of a leaderboard pipeline, used by leaderboard nodes and data connections.
///
/// Steps get evaluated again whenever their leaderboard is read, also for competitions which are long over.
/// So what a variant does must never change, a change is a new variant (like `SelectV2`) next to the old one.
#[derive(Debug, SpacetimeType, Clone)]
pub(crate) enum LbSettingsV3 {
    /// Turns the rounds of every match into the standings of that match.
    Match,
    Merge(LbMergeSettingsV2),
    Filter(LbFilterSettingsV2),
    Remap(LbRemapSettings),
    Select(LbSelectSettings),
    Sort(LbSortSettings),
    Renumber(LbRenumberSettings),
}

/// Takes the entries of a leaderboard and returns the changed ones.
pub(crate) trait LbStep {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError>;
    /// Whether the step works on the entries it would get, returns what it would produce.
    /// Only steps which certainly fail or produce something meaningless are rejected.
    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError>;
}

impl LbStep for LbSettingsV3 {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        match self {
            LbSettingsV3::Match => match_standings(cx, entries),
            LbSettingsV3::Merge(settings) => settings.apply(cx, entries),
            LbSettingsV3::Filter(settings) => settings.apply(cx, entries),
            LbSettingsV3::Remap(settings) => settings.apply(cx, entries),
            LbSettingsV3::Select(settings) => settings.apply(cx, entries),
            LbSettingsV3::Sort(settings) => settings.apply(cx, entries),
            LbSettingsV3::Renumber(settings) => settings.apply(cx, entries),
        }
    }

    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        match self {
            // Standings stay as they are, so this is never wrong.
            LbSettingsV3::Match => Ok(LbShape {
                rounds: false,
                standings: !input.is_empty(),
                ..input
            }),
            LbSettingsV3::Merge(settings) => settings.check(origin, input),
            LbSettingsV3::Filter(settings) => settings.check(origin, input),
            LbSettingsV3::Remap(settings) => settings.check(origin, input),
            LbSettingsV3::Select(settings) => settings.check(origin, input),
            LbSettingsV3::Sort(settings) => settings.check(origin, input),
            LbSettingsV3::Renumber(settings) => settings.check(origin, input),
        }
    }
}

/// Runs the steps one after another, entries created by a step belong to the origin of `cx`.
pub(crate) fn evaluate_steps(
    cx: &LbContext,
    steps: &[LbSettingsV3],
    entries: Vec<LbEntry>,
) -> Result<Vec<LbEntry>, LbError> {
    steps
        .iter()
        .enumerate()
        .try_fold(entries, |entries, (index, step)| {
            step.apply(cx, entries)
                .map_err(|error| error.at_step(cx.origin(), index))
        })
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbParams {
    Score,
    Time,
    Position,
}

impl LbParams {
    /// Sorting by the key puts the best entries first.
    fn key(self, entry: &LbEntry) -> i64 {
        match self {
            LbParams::Score => -(entry.score as i64),
            LbParams::Time => LbParams::time_key(entry.time) as i64,
            LbParams::Position => entry.position as i64,
        }
    }

    /// A time of zero or less was not driven so it is the worst.
    fn time_key(time: i32) -> i32 {
        if time <= 0 { i32::MAX } else { time }
    }
}

/// What the entries are grouped by so a step is applied to every group on its own.
#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LbKey {
    /// The node the entries come from, e.g. every match.
    Origin,
    Map,
    Round,
    Team,
}

/// The values of the keys in a scope, keys which are not part of it stay empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
struct LbScopeKey {
    origin: Option<NodeHandle>,
    map_id: Option<u32>,
    round: Option<u16>,
    team_id: Option<u32>,
}

impl LbScopeKey {
    fn of(scope: &[LbKey], entry: &LbEntry) -> Self {
        let mut key = LbScopeKey::default();
        for scope_key in scope {
            match scope_key {
                LbKey::Origin => key.origin = Some(entry.get_node()),
                LbKey::Map => key.map_id = Some(entry.map_id),
                LbKey::Round => key.round = Some(entry.round),
                LbKey::Team => key.team_id = Some(entry.team_id),
            }
        }
        key
    }
}

/// The map entries were played on together, 0 if they were played on different ones.
fn shared_map(a: u32, b: u32) -> u32 {
    if a == b { a } else { 0 }
}

/// Whom an entry ranks, a leaderboard has at most one standing per participant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LbParticipant {
    Player(u32),
    Team(u32),
}

/// Whether an entry is the result of a single round or the standing in a whole leaderboard.
#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LbEntryKind {
    /// Comes from a match, its mode decides how the rounds become standings.
    Round,
    Standing,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub struct LbEntry {
    /// The user the entry belongs to, 0 if there is none.
    /// Having this as 0 means team_id cannot be 0.
    pub user_id: u32,
    /// The team the entry belongs to, 0 if there is none.
    /// Without a user the entry is the one of the team.
    /// Having this as 0 user_id cannot be 0.
    pub team_id: u32,
    // Required
    pub position: u16,
    // Default: 0
    pub round: u16,
    // Default: 0, which is an unknown map or several ones.
    pub map_id: u32,
    // Default: i32::MIN
    pub score: i32,
    // Default: i32::MIN
    pub time: i32,
    // Required
    node_id: u32,
    // Required
    node_variant: u8,
    // Required
    pub kind: LbEntryKind,
}

impl LbEntry {
    pub(crate) fn new(
        user_id: u32,
        kind: LbEntryKind,
        position: u16, /* , origin_idx: u16 */
        origin: NodeHandle,
    ) -> Self {
        let (node_variant, node_id) = origin.split();
        LbEntry {
            user_id,
            position,
            kind,
            round: 0,
            map_id: 0,
            score: i32::MIN,
            time: i32::MIN,
            node_id,
            node_variant,
            team_id: 0, //TODO
        }
    }

    pub(crate) fn set_score(mut self, score: i32) -> Self {
        self.score = score;
        self
    }

    pub(crate) fn set_time(mut self, time: i32) -> Self {
        self.time = time;
        self
    }

    /// An entry of the participant without any results.
    pub(crate) fn for_participant(
        participant: LbParticipant,
        kind: LbEntryKind,
        position: u16,
        origin: NodeHandle,
    ) -> Self {
        match participant {
            LbParticipant::Player(user_id) => LbEntry::new(user_id, kind, position, origin),
            LbParticipant::Team(team_id) => {
                let mut entry = LbEntry::new(0, kind, position, origin);
                entry.team_id = team_id;
                entry
            }
        }
    }

    pub(crate) fn set_round(mut self, round: u16) -> Self {
        self.round = round;
        self
    }

    pub(crate) fn set_map(mut self, map_id: u32) -> Self {
        self.map_id = map_id;
        self
    }

    pub(crate) fn participant(&self) -> LbParticipant {
        if self.user_id != 0 {
            // The team is left out so a player who played for several teams still has one standing.
            LbParticipant::Player(self.user_id)
        } else {
            LbParticipant::Team(self.team_id)
        }
    }

    pub(crate) fn get_user(&self) -> u32 {
        self.user_id
    }

    /// The entry as one of the leaderboard of `origin`.
    pub(crate) fn with_origin(mut self, origin: NodeHandle) -> Self {
        (self.node_variant, self.node_id) = origin.split();
        self
    }

    pub(crate) fn get_node(&self) -> NodeHandle {
        NodeHandle::combine(self.node_variant, self.node_id)
    }
}

#[cfg(test)]
mod tests {
    use tm_server_types::config::TmMode;

    use super::*;
    use crate::pipeline::{
        steps::select::{LbSelectPlaces, LbSelectSettings},
        test_util::{round, test_context},
    };

    #[test]
    fn errors_name_the_node_and_step_which_failed() {
        let test = test_context(&[(1, TmMode::ReverseCup)]);
        let leaderboard = NodeHandle::LeaderboardV1(3);

        // The reverse cup has no config to take its starting points from.
        let error = evaluate_steps(
            &LbContext::new(&test.anonymous_view_context(), leaderboard),
            &[
                LbSettingsV3::Select(LbSelectSettings {
                    scope: Vec::new(),
                    places: LbSelectPlaces::FirstN(5),
                }),
                LbSettingsV3::Match,
            ],
            vec![round(1, 1, -500, 0)],
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "LeaderboardV1(3) step 2: Could not find the reverse cup config of MatchV1(1)."
        );
    }
}
