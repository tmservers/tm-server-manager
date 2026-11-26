use std::fmt;

use crate::competition::node::NodeHandle;

/// Why a leaderboard could not be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LbError {
    /// The node whose steps failed.
    node: Option<NodeHandle>,
    /// The data connection whose steps failed, its origin is in `node`.
    connection: Option<u32>,
    /// The index of the step which failed.
    step: Option<usize>,
    message: String,
}

impl LbError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        LbError {
            node: None,
            connection: None,
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

    /// Marks that the steps of the data connection failed, they run with its origin.
    pub(crate) fn in_connection(mut self, connection_id: u32) -> Self {
        if self.connection.is_none() {
            self.connection = Some(connection_id);
        }
        self
    }
}

impl fmt::Display for LbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.connection, self.node, self.step) {
            (Some(connection), _, Some(step)) => {
                write!(
                    f,
                    "Connection {connection} step {}: {}",
                    step + 1,
                    self.message
                )
            }
            (None, Some(node), Some(step)) => {
                write!(f, "{node:?} step {}: {}", step + 1, self.message)
            }
            _ => f.write_str(&self.message),
        }
    }
}

impl From<LbError> for String {
    fn from(error: LbError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_innermost_step_when_passed_up() {
        // A leaderboard failing inside of the input of another one.
        let error = LbError::new("broken")
            .at_step(NodeHandle::LeaderboardV1(1), 0)
            .at_step(NodeHandle::LeaderboardV1(2), 3);

        assert_eq!(error.to_string(), "LeaderboardV1(1) step 1: broken");
    }
}
