use std::fmt;

use crate::competition::node::NodeHandle;

/// Why a leaderboard could not be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LbError {
    /// The node whose steps failed.
    node: Option<NodeHandle>,
    /// The index of the step which failed.
    step: Option<usize>,
    message: String,
}

impl LbError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        LbError {
            node: None,
            step: None,
            message: message.into(),
        }
    }

    /// Marks where the error happened, an error which already knows its step keeps it.
    pub(crate) fn at_step(mut self, node: NodeHandle, step: usize) -> Self {
        if self.step.is_none() {
            self.node = Some(node);
            self.step = Some(step);
        }
        self
    }
}

impl fmt::Display for LbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.node, self.step) {
            (Some(node), Some(step)) => write!(f, "{node:?} step {}: {}", step + 1, self.message),
            _ => f.write_str(&self.message),
        }
    }
}

impl From<LbError> for String {
    fn from(error: LbError) -> Self {
        error.to_string()
    }
}
