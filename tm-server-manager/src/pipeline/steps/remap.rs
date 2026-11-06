use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    pipeline::{LbEntry, LbParams, LbStep, context::LbContext, error::LbError, shape::LbShape},
};

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbRemapSettings {
    origin: LbParams,
    target: LbParams,
    manipulation: LbManipulationKind,
    manipulation_value: i32,
}

/// Lhs and Rhs are thought from the origin
#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbManipulationKind {
    Add,
    Multiply,
    SubtractLhs,
    SubtractRhs,
    DivideLhs,
    DivideRhs,
}

impl LbStep for LbRemapSettings {
    fn apply(&self, _cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        Ok(self.evaluate(entries))
    }

    fn check(&self, _origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        if matches!(self.manipulation, LbManipulationKind::DivideLhs)
            && self.manipulation_value == 0
        {
            return Err(LbError::new("Cannot divide by 0."));
        }
        Ok(input)
    }
}

impl LbRemapSettings {
    /// Only changes values, so the entries keep their origin and can still be matched afterwards.
    /// Results saturate instead of overflowing.
    /// Entries without a value to remap, or which would be divided by 0, stay as they are.
    fn evaluate(self, mut leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        for entry in &mut leaderboard {
            let Some(new_val) = self.remapped(entry) else {
                continue;
            };

            match self.target {
                LbParams::Score => entry.score = new_val,
                LbParams::Time => entry.time = new_val,
                LbParams::Position => entry.position = new_val.clamp(0, u16::MAX as i32) as u16,
            }
        }
        leaderboard
    }

    fn remapped(self, entry: &LbEntry) -> Option<i32> {
        let origin = match self.origin {
            // A score of i32::MIN was never set.
            LbParams::Score if entry.score == i32::MIN => return None,
            LbParams::Score => entry.score,
            // A time of zero or less was not driven, a penalty must not turn it into a driven one.
            LbParams::Time if entry.time <= 0 => return None,
            LbParams::Time => entry.time,
            LbParams::Position => entry.position as i32,
        };
        let value = self.manipulation_value;
        match self.manipulation {
            LbManipulationKind::Add => Some(origin.saturating_add(value)),
            LbManipulationKind::Multiply => Some(origin.saturating_mul(value)),
            LbManipulationKind::SubtractLhs => Some(origin.saturating_sub(value)),
            LbManipulationKind::SubtractRhs => Some(value.saturating_sub(origin)),
            LbManipulationKind::DivideLhs => (value != 0).then(|| origin.saturating_div(value)),
            LbManipulationKind::DivideRhs => (origin != 0).then(|| value.saturating_div(origin)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::test_util::round;

    #[test]
    fn rhs_operations_use_the_origin_as_right_hand_side() {
        let remapped = LbRemapSettings {
            origin: LbParams::Score,
            target: LbParams::Time,
            manipulation: LbManipulationKind::SubtractRhs,
            manipulation_value: 100,
        }
        .evaluate(vec![round(1, 1, 30, 5000)]);

        assert_eq!(remapped[0].time, 70);
        assert_eq!(remapped[0].score, 30);
    }

    #[test]
    fn an_entry_which_would_be_divided_by_zero_stays_as_it_is() {
        let remapped = LbRemapSettings {
            origin: LbParams::Score,
            target: LbParams::Score,
            manipulation: LbManipulationKind::DivideRhs,
            manipulation_value: 1000,
        }
        .evaluate(vec![round(1, 1, 10, 0), round(2, 1, 0, 0)]);

        let scores: Vec<i32> = remapped.iter().map(|e| e.score).collect();
        assert_eq!(scores, vec![100, 0]);
    }

    #[test]
    fn unset_scores_and_times_which_were_not_driven_stay_as_they_are() {
        let penalty = |origin| LbRemapSettings {
            origin,
            target: origin,
            manipulation: LbManipulationKind::Add,
            manipulation_value: 1000,
        };
        let not_driven = round(1, 1, 0, 0);
        let unset_score = LbEntry {
            score: i32::MIN,
            ..not_driven
        };

        assert_eq!(
            penalty(LbParams::Time).evaluate(vec![not_driven])[0].time,
            0
        );
        assert_eq!(
            penalty(LbParams::Score).evaluate(vec![unset_score])[0].score,
            i32::MIN
        );
    }

    #[test]
    fn results_saturate_instead_of_overflowing() {
        let remapped = LbRemapSettings {
            origin: LbParams::Score,
            target: LbParams::Position,
            manipulation: LbManipulationKind::SubtractRhs,
            manipulation_value: 0,
        }
        .evaluate(vec![round(1, 1, 10, 0)]);

        // A negative place is not possible, so it becomes the best one.
        assert_eq!(remapped[0].position, 0);
    }
}
