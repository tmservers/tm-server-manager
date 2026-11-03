use std::collections::HashMap;

use spacetimedb::SpacetimeType;
use tm_server_types::config::TmMode;

use crate::{
    competition::node::NodeHandle,
    leaderboard::{LbEntry, LbParams, group_by, renumber_positions},
};

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbMergeSettings {
    /// This whole thing should be considered deprecated it has no meaning i think.
    kind: LbMergeKind,
    action: LbMergeAction,
    param: LbParams,
}

/// This whole thing should be considered deprecated it has no meaning i think.
#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbMergeKind {
    Rounds,
}

// Is this only for collisions?
#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbMergeAction {
    Average,
    Summate,
}

impl LbMergeSettings {
    pub(super) fn evaluate(self, origin: NodeHandle, leaderboard: Vec<LbEntry>) -> Vec<LbEntry> {
        let player_rounds = group_by(leaderboard, |e| e.user_id);

        let mut map: HashMap<u32, LbEntry> = HashMap::new();
        for (player, rounds) in player_rounds {
            let num_rounds = rounds.len();
            //TODO set right position for this whole thing.
            let init = LbEntry::new(player, TmMode::Unknown, 0, origin)
                .set_score(0)
                .set_time(0);
            let mut accumulated = rounds.into_iter().fold(init, |mut acc, x| {
                match self.param {
                    LbParams::Score => acc.score += x.score,
                    LbParams::Time => acc.time += x.time,
                    LbParams::Position => acc.position += x.position,
                }
                acc
            });
            if let LbMergeAction::Average = self.action {
                accumulated.score = (accumulated.score as f32 / num_rounds as f32) as i32;
            }

            map.insert(player, accumulated);
        }
        log::info!("Merge Map Result: {:?}", map);

        let mut vec = map.into_values().collect::<Vec<_>>();
        self.param.sort(&mut vec);
        if let LbParams::Score = self.param {
            // readjust the position.
            renumber_positions(&mut vec);
        }

        vec
    }
}
