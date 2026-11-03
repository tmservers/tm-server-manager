use spacetimedb::SpacetimeType;

use crate::leaderboard::{LbEntry, LbParams};

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

impl LbRemapSettings {
    /// Only changes values, so the entries keep their origin and can still be matched afterwards.
    pub(super) fn evaluate(self, mut leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        for entry in &mut leaderboard {
            let new_val = match self.manipulation {
                LbManipulationKind::Add => match self.origin {
                    LbParams::Score => entry.score + self.manipulation_value,
                    LbParams::Time => entry.time + self.manipulation_value,
                    LbParams::Position => entry.position as i32 + self.manipulation_value,
                },
                LbManipulationKind::Multiply => match self.origin {
                    LbParams::Score => entry.score * self.manipulation_value,
                    LbParams::Time => entry.time * self.manipulation_value,
                    LbParams::Position => entry.position as i32 * self.manipulation_value,
                },
                LbManipulationKind::SubtractLhs => match self.origin {
                    LbParams::Score => entry.score - self.manipulation_value,
                    LbParams::Time => entry.time - self.manipulation_value,
                    LbParams::Position => entry.position as i32 - self.manipulation_value,
                },
                LbManipulationKind::SubtractRhs => match self.origin {
                    LbParams::Score => self.manipulation_value - entry.score,
                    LbParams::Time => self.manipulation_value - entry.time,
                    LbParams::Position => self.manipulation_value - entry.position as i32,
                },
                LbManipulationKind::DivideLhs => match self.origin {
                    LbParams::Score => entry.score / self.manipulation_value,
                    LbParams::Time => entry.time / self.manipulation_value,
                    LbParams::Position => entry.position as i32 / self.manipulation_value,
                },
                LbManipulationKind::DivideRhs => match self.origin {
                    LbParams::Score => self.manipulation_value / entry.score,
                    LbParams::Time => self.manipulation_value / entry.time,
                    LbParams::Position => self.manipulation_value / entry.position as i32,
                },
            };

            match self.target {
                LbParams::Score => entry.score = new_val,
                LbParams::Time => entry.time = new_val,
                LbParams::Position => entry.position = new_val as u16,
            }
        }
        leaderboard
    }
}
