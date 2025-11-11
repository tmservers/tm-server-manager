mod chat;
pub use chat::*;

// TODO: Probably rename to MethodCall
#[derive(Debug, Clone)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "spacetime", derive(spacetimedb_lib::SpacetimeType))]
#[cfg_attr(feature = "spacetime", sats(crate = spacetimedb_lib))]
pub enum MethodCall {
    /// ===============
    /// XML-RPC Methods
    /// ===============
    ///
    ListMethods,
    MethodSignature(String),
    MethodHelp(String),
    ChatSendServerMessage(String),
    ChatSendServerMessageToUser(ChatSendServerMessageToUserArgs),
    ChatSend(String),
    ChatSendToUser(ChatSendToUserArgs),

    Kick(KickArgs),
    Ban(BanArgs),
    UnBan(String),

    Ignore(String),
    UnIgnore(String),

    SetPlayerPassword(String),
    SetSpectatorPassword(String),

    SendToServerAfterMatchEnd(String),

    /// ===============
    /// ModeScript Methods
    /// ===============
    ///
    GetMethodsList,

    PauseSetActive(bool),
}
