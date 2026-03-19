use spacetimedb::{SpacetimeType, table};

#[table(accessor=tab_connection_action)]
pub struct TabConnectionAction {
    #[index(hash)]
    competition_id: u32,
    #[primary_key]
    pub connection_id: u32,

    action: ConnectionAction,
}

// Versioning works be e.g.:
// MatchV1A2(ConnectionActionMatchV2)
#[derive(Debug, SpacetimeType)]
enum ConnectionAction {
    MatchV1(ConnectionActionMatch),
    RegistrationV1(ConnectionActionRegistration),
}

#[derive(Debug, SpacetimeType)]
enum ConnectionActionMatch {
    TryStart,
    ForceStart,
}

#[derive(Debug, SpacetimeType)]
enum ConnectionActionRegistration {
    Open,
    Close,
}
