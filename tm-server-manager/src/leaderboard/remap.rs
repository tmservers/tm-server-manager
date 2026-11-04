use spacetimedb::SpacetimeType;

use crate::{
    competition::node::NodeHandle,
    leaderboard::{LbEntry, LbParams, LbStep, context::LbContext, error::LbError, shape::LbShape},
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
        self.evaluate(entries)
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
    fn evaluate(self, mut leaderboard: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        let value = self.manipulation_value;
        for entry in &mut leaderboard {
            let origin = match self.origin {
                LbParams::Score => entry.score,
                LbParams::Time => entry.time,
                LbParams::Position => entry.position as i32,
            };
            let new_val = match self.manipulation {
                LbManipulationKind::Add => origin.checked_add(value),
                LbManipulationKind::Multiply => origin.checked_mul(value),
                LbManipulationKind::SubtractLhs => origin.checked_sub(value),
                LbManipulationKind::SubtractRhs => value.checked_sub(origin),
                LbManipulationKind::DivideLhs => origin.checked_div(value),
                LbManipulationKind::DivideRhs => value.checked_div(origin),
            }
            .ok_or_else(|| {
                LbError::new(format!(
                    "Cannot remap {origin} with {:?} {value}.",
                    self.manipulation
                ))
            })?;

            match self.target {
                LbParams::Score => entry.score = new_val,
                LbParams::Time => entry.time = new_val,
                LbParams::Position => entry.position = new_val as u16,
            }
        }
        Ok(leaderboard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::leaderboard::test_util::round;

    #[test]
    fn rhs_operations_use_the_origin_as_right_hand_side() {
        let remapped = LbRemapSettings {
            origin: LbParams::Score,
            target: LbParams::Time,
            manipulation: LbManipulationKind::SubtractRhs,
            manipulation_value: 100,
        }
        .evaluate(vec![round(1, 1, 30, 5000)])
        .unwrap();

        assert_eq!(remapped[0].time, 70);
        assert_eq!(remapped[0].score, 30);
    }

    #[test]
    fn dividing_by_zero_is_an_error() {
        let remapped = LbRemapSettings {
            origin: LbParams::Score,
            target: LbParams::Score,
            manipulation: LbManipulationKind::DivideRhs,
            manipulation_value: 1000,
        }
        .evaluate(vec![round(1, 1, 10, 0), round(2, 1, 0, 0)]);

        assert!(remapped.is_err());
    }
}
