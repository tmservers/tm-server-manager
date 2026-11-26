//! What the host tells a mode besides its events.

use crate::{Team, User};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// What a mode can ask the host for while it handles an event.
///
/// Everything it answers follows from the inputs the driver got, so a replay of
/// the match answers the same.
pub trait Host {
    fn log(&self, level: LogLevel, message: &str);

    /// What the host knows about a player, e.g. the name for a chat message.
    fn user(&self, account_id: &str) -> Option<User>;

    /// Maps in the playlist of the match, 0 without one.
    fn map_count(&self) -> u32;
    /// The teams of a match of teams, empty for a match of players.
    fn teams(&self) -> Vec<Team> {
        Vec::new()
    }
    /// The team a player plays for.
    fn team_of(&self, account_id: &str) -> Option<u32> {
        self.teams()
            .into_iter()
            .find(|team| team.players.iter().any(|player| player == account_id))
            .map(|team| team.team_id)
    }

    /// The name of a player, e.g. for chat messages. The account id while the host
    /// does not know the name.
    fn name(&self, account_id: &str) -> String {
        self.user(account_id)
            .map(|user| user.name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| account_id.into())
    }
}
