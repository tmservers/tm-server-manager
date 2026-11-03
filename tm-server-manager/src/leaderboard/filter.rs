use spacetimedb::SpacetimeType;
use tm_server_types::config::TmMode;

use crate::{
    competition::node::{NodeHandle, NodeLeaderboard},
    leaderboard::{LbEntry, LbParams, group_by},
};

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub(crate) struct LbFilterSettings {
    kind: LbFilterKind,
    param: LbParams,
    filter: LbFilterIdent,
    fallback: LbFilterFallback,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbFilterIdent {
    Best(u16),
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbFilterFallback {
    Worst,
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum LbFilterKind {
    //Maps,
    Rounds,
    // Actions // TODO we should be able to inspect the actions of the player.
    Matches,
}

impl LbFilterSettings {
    pub(super) fn evaluate(
        self,
        origin: NodeHandle,
        leaderboard: Vec<LbEntry>,
        ctx: &impl spacetimedb::CtxDbRead,
    ) -> Vec<LbEntry> {
        let (mut map, max) = match self.kind {
            //LbFilterKind::Match => todo!(),
            //LbFilterKind::Maps => todo!(),
            //TODO valiidate that the sort does all edge cases.
            LbFilterKind::Rounds => {
                let map = group_by(leaderboard, |e| e.user_id);

                let Some(max_rounds) = map.values().map(Vec::len).max() else {
                    // This case is only hit when nobody is even there so it is safe to just return nothing.
                    return Vec::new();
                };
                (map, max_rounds as u16)
            }
            LbFilterKind::Matches => {
                let lb_nodes = group_by(leaderboard, |e| e.get_node());
                let max = lb_nodes.len() as u16;

                let results = lb_nodes.values().flat_map(|entries| entries.finalize(ctx));
                (group_by(results, |e| e.get_user()), max)
            }
        };

        let map_len = map.len();

        if map_len == 0 {
            return Vec::new();
        }

        for rows in map.values_mut() {
            self.param.sort(rows);
        }

        for (user_id, rows) in &mut map {
            match self.filter {
                LbFilterIdent::Best(n) => {
                    rows.truncate(n as usize);
                    if rows.len() < (n as usize) {
                        // if we have played less rounds than the best n we take the current row count.
                        let needed = if n > max { max } else { n };
                        let missing = needed - rows.len() as u16;
                        match self.fallback {
                            LbFilterFallback::Worst => match self.param {
                                LbParams::Score => {
                                    for _ in 0..missing {
                                        //TODO consider mode
                                        rows.push(
                                            LbEntry::new(
                                                *user_id,
                                                TmMode::Unknown,
                                                map_len as u16,
                                                origin,
                                            )
                                            .set_score(0),
                                        );
                                    }
                                }
                                LbParams::Time => (), //TODO
                                LbParams::Position => {
                                    for _ in 0..missing {
                                        rows.push(LbEntry::new(
                                            *user_id,
                                            TmMode::Unknown,
                                            map_len as u16,
                                            origin,
                                        ));
                                    }
                                }
                            },
                        }
                    }
                }
            };
        }

        let filter = map.into_values().flatten().collect();

        filter
    }
}

// filter per: (everything specific to user) | think it the other way around? if i request a filter rounds we automatically separate by maps?
// matches -> outputs match | invalid for filter
// match -> outputs maps | maps can either be map count or map id i guess e.g. if map is played 2 times.
// map -> outputs rounds
// rounds -> outputs params

// Input: its always the MatchRoundPlayer.
// that means that we always sort by player???????

// keyed/ unkeyed filter?
// a big problem are the orphaned fields which proceed to be meaningless when remapping stuff 🤔
// position should always be recomputed i guess
// the match is a problem when propagating... this is because the leadearboard is now also something which is able to be searched for 🤔
// this means the connection should give me the leaderboard and then we also shoud do a new struct i reckon.

// separate by map
// for each map filter rounds best 5/6 merge rounds averge position

// match is match or leadarboard or probably also monitoring in the future.
// how to incorporate this? map_id: u32, | for match its pf for leaderboard its harder.
