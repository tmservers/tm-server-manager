//! What is known about the entries of a pipeline before they exist, so its steps can be checked when they get saved.

use crate::{
    competition::node::NodeHandle,
    leaderboard::{LbKey, LbSettingsV3, LbStep, error::LbError},
};

/// The nodes the entries can come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum LbOrigins {
    #[default]
    None,
    One(NodeHandle),
    Many,
}

/// The entries a pipeline can get, the default is getting none at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LbShape {
    /// Entries can be rounds of a match.
    pub(crate) rounds: bool,
    /// Entries can be standings.
    pub(crate) standings: bool,
    pub(crate) origins: LbOrigins,
    /// Entries can carry the map they were played on.
    pub(crate) maps: bool,
}

impl LbShape {
    /// The rounds of a match.
    pub(crate) fn rounds(origin: NodeHandle) -> Self {
        LbShape {
            rounds: true,
            standings: false,
            origins: LbOrigins::One(origin),
            maps: true,
        }
    }

    /// Standings without any maps, like the ones of a registration.
    pub(crate) fn standings(origin: NodeHandle) -> Self {
        LbShape {
            rounds: false,
            standings: true,
            origins: LbOrigins::One(origin),
            maps: false,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.rounds && !self.standings
    }

    /// The entries of both together, like the inputs of a node.
    pub(crate) fn combine(self, other: LbShape) -> Self {
        let origins = match (self.origins, other.origins) {
            (LbOrigins::None, origins) | (origins, LbOrigins::None) => origins,
            (LbOrigins::One(a), LbOrigins::One(b)) if a == b => LbOrigins::One(a),
            _ => LbOrigins::Many,
        };
        LbShape {
            rounds: self.rounds || other.rounds,
            standings: self.standings || other.standings,
            origins,
            maps: self.maps || other.maps,
        }
    }
}

/// Checks the steps against the entries they would get and returns what they produce.
/// Entries created by a step belong to `origin`, like when evaluating them.
pub(crate) fn check_steps(
    origin: NodeHandle,
    steps: &[LbSettingsV3],
    input: LbShape,
) -> Result<LbShape, LbError> {
    steps
        .iter()
        .enumerate()
        .try_fold(input, |shape, (index, step)| {
            step.check(origin, shape)
                .map_err(|error| error.at_step(origin, index))
        })
}

/// A scope can only group by values the entries have.
pub(super) fn check_scope(scope: &[LbKey], input: &LbShape) -> Result<(), LbError> {
    if scope.contains(&LbKey::Map) && !input.is_empty() && !input.maps {
        return Err(LbError::new("The entries have no map to group them by."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leaderboard::{
        LbKey,
        select::{LbSelectPlaces, LbSelectSettings},
        test_util::MATCH,
    };

    fn select(scope: Vec<LbKey>) -> LbSettingsV3 {
        LbSettingsV3::Select(LbSelectSettings {
            scope,
            places: LbSelectPlaces::FirstN(3),
        })
    }

    #[test]
    fn selecting_rounds_needs_a_match_step_first() {
        let rounds = LbShape::rounds(MATCH);

        assert!(check_steps(MATCH, &[select(vec![])], rounds).is_err());
        assert!(check_steps(MATCH, &[LbSettingsV3::Match, select(vec![])], rounds).is_ok());
    }

    #[test]
    fn selecting_from_several_origins_needs_an_origin_scope() {
        let rounds = LbShape::rounds(MATCH).combine(LbShape::rounds(NodeHandle::MatchV1(2)));

        assert!(check_steps(MATCH, &[LbSettingsV3::Match, select(vec![])], rounds).is_err());
        assert!(
            check_steps(
                MATCH,
                &[LbSettingsV3::Match, select(vec![LbKey::Origin])],
                rounds
            )
            .is_ok()
        );
    }

    #[test]
    fn a_map_scope_needs_entries_with_maps() {
        let registration = LbShape::standings(NodeHandle::RegistrationV1(1));

        assert!(check_steps(MATCH, &[select(vec![LbKey::Map])], registration).is_err());
    }

    #[test]
    fn nothing_is_rejected_before_there_are_inputs() {
        // Steps can be configured before the inputs get connected.
        assert!(check_steps(MATCH, &[select(vec![LbKey::Map])], LbShape::default()).is_ok());
    }
}
