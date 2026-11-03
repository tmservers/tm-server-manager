use crate::{
    competition::node::NodeHandle,
    leaderboard::{LbEntry, LbEntryKind},
};

pub(crate) const MATCH: NodeHandle = NodeHandle::MatchV1(1);

pub(crate) fn round(user_id: u32, round: u16, score: i32, time: i32) -> LbEntry {
    LbEntry::new(user_id, LbEntryKind::Round, 0, MATCH)
        .set_round(round)
        .set_score(score)
        .set_time(time)
}

pub(crate) fn standing(user_id: u32, position: u16) -> LbEntry {
    LbEntry::new(user_id, LbEntryKind::Standing, position, MATCH)
}

/// The users with their positions, in the order of the entries.
pub(crate) fn ranking(entries: &[LbEntry]) -> Vec<(u32, u16)> {
    entries.iter().map(|e| (e.user_id, e.position)).collect()
}
