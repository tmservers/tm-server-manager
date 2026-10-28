//! What surrounds the driver in a simulation: the server with its maps and
//! players, how they race and what goes wrong. Random from a seed or written down
//! as a scenario.

use std::collections::BTreeMap;

use crate::{Mode, Rng};

#[derive(Debug, Clone)]
pub struct World {
    pub mode: Mode,
    /// The json settings of the mode.
    pub settings: String,
    /// The map list of the server. The first map runs when the driver loads.
    pub maps: Vec<MapSpec>,
    /// The playlist the driver gets when it loads, empty for none.
    pub playlist: Vec<String>,
    /// Everyone who is on the server at some point.
    pub players: Vec<PlayerSpec>,
    pub races: Races,
    pub faults: Faults,
    /// Matches that end before the simulation does.
    pub matches: u32,
    /// Rounds after which the simulation ends anyway, e.g. for a match without an end.
    pub max_rounds: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSpec {
    pub uid: String,
    /// The race time of every sector at the pace of the author in milliseconds. The
    /// last sector ends at the finish.
    pub sectors: Vec<u32>,
}

impl MapSpec {
    pub fn author_time(&self) -> u32 {
        self.sectors.iter().sum()
    }

    /// The race times at the checkpoints at the pace of the author, the finish last.
    pub fn author_checkpoints(&self) -> Vec<u32> {
        self.sectors
            .iter()
            .scan(0, |time, sector| {
                *time += sector;
                Some(*time)
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PlayerSpec {
    pub account_id: String,
    pub name: String,
    /// How fast random races are in per mille of the pace of the author, e.g. 1050
    /// is 5 % slower.
    pub pace: u32,
    /// How much a sector of a random race varies in milliseconds.
    pub spread: u32,
    /// Chance to give up a random race in per mille.
    pub give_up: u32,
    /// None for players on the server when the driver loads.
    pub joins: Option<Moment>,
    pub leaves: Option<Moment>,
    /// Only watches and never races.
    pub watches: bool,
}

/// A round of the simulation, counted from 1 over every map and match, and whether
/// something happens while it runs or before it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    pub round: u32,
    pub during: bool,
}

#[derive(Debug, Clone)]
pub enum Races {
    /// Players race at their pace, as many rounds as the match takes.
    Random {
        /// Chance of a round with two equal races in per mille.
        ties: u32,
        /// Chance that a player does not move in a race in per mille.
        idle: u32,
    },
    /// The rounds of a scenario, the simulation ends after the last one.
    Scripted(Vec<ScriptedRound>),
}

#[derive(Debug, Clone, Default)]
pub struct ScriptedRound {
    /// What happens before the round starts.
    pub before: Vec<Action>,
    /// An admin ends the round once the races are over, so it does not count.
    pub forced: bool,
    /// The races of the players in order. Players who could race but are missing do
    /// not move.
    pub races: BTreeMap<String, Vec<RacePlan>>,
}

/// A race as it is going to happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RacePlan {
    /// Reaches the finish with these race times at the checkpoints, the finish last.
    Finish(Vec<u32>),
    /// Reaches the finish in this time, as much faster or slower in every sector
    /// than the author on the map it is driven on.
    FinishIn(u32),
    /// Gives up after passing checkpoints at these race times.
    GiveUp(Vec<u32>),
    /// Gives up after passing this many checkpoints at the pace of the author on the
    /// map it is driven on.
    GiveUpAfter(usize),
    /// Spawns but does not move.
    Idle,
}

impl RacePlan {
    /// The race on a map, with the race times at the checkpoints known.
    pub fn on(&self, map: &MapSpec) -> RacePlan {
        match self {
            RacePlan::FinishIn(time) => {
                let author = u64::from(map.author_time().max(1));
                let checkpoints = map
                    .author_checkpoints()
                    .iter()
                    .map(|&at| (u64::from(at) * u64::from(*time) / author) as u32)
                    .collect();
                RacePlan::Finish(checkpoints)
            }
            RacePlan::GiveUpAfter(passed) => {
                let mut checkpoints = map.author_checkpoints();
                // Giving up never reaches the finish.
                checkpoints.truncate((*passed).min(checkpoints.len().saturating_sub(1)));
                RacePlan::GiveUp(checkpoints)
            }
            plan => plan.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Join(String),
    Leave(String),
    /// From now on every hold reaches the driver twice, like when the modescript
    /// asks again before the answer arrived.
    RepeatHolds(bool),
    /// The bridge restarts and resumes the driver by replaying its inputs.
    RestartBridge,
    /// The modescript restarts, which starts a new match.
    RestartScript,
    /// The driver gets a new playlist.
    Playlist(Vec<String>),
}

/// How often things go wrong, all in per mille.
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// Holds that reach the driver twice.
    pub repeated_holds: u32,
    /// Rounds an admin forces to end.
    pub forced_rounds: u32,
    /// Start lines that do not reach the driver.
    pub missed_starts: u32,
    /// Rounds in which the bridge restarts and resumes the driver.
    pub bridge_restarts: u32,
    /// Rounds before which the modescript restarts.
    pub script_restarts: u32,
    /// Rounds before which the driver gets another playlist.
    pub playlist_changes: u32,
}

const NAMES: [&str; 16] = [
    "Ant", "Bea", "Cid", "Dot", "Eve", "Fay", "Gus", "Hal", "Ivy", "Jo", "Kai", "Lu", "Max", "Ned",
    "Oli", "Pia",
];

impl World {
    /// A world for a mode where everything varies with the seed: the settings of the
    /// mode, the maps, how many players there are, how they race, when they come
    /// and go and what goes wrong. Most worlds are plain, some have every fault.
    pub fn random(mode: Mode, seed: u64) -> World {
        let rng = Rng::new(seed);
        let settings = mode.random_settings(&mut rng.fork("settings"));
        let mut rng = rng.fork("world");

        let maps: Vec<MapSpec> = (1..=rng.between(1, 3))
            .map(|n| MapSpec {
                uid: format!("m{n}"),
                sectors: (0..rng.between(2, 5))
                    .map(|_| rng.between(3_000, 12_000))
                    .collect(),
            })
            .collect();
        let mut playlist: Vec<String> = maps.iter().map(|map| map.uid.clone()).collect();
        match rng.below(4) {
            0 => playlist.clear(),
            1 => {
                rng.shuffle(&mut playlist);
                playlist.truncate(rng.between(1, playlist.len() as u32) as usize);
            }
            _ => {}
        }

        let count = match rng.below(100) {
            0..=1 => 0,
            2..=6 => 1,
            7..=36 => rng.between(2, 4),
            37..=76 => rng.between(5, 8),
            _ => rng.between(9, 16),
        };
        let players = (0..count as usize)
            .map(|index| {
                let name = match rng.below(20) {
                    0 => format!("$f00{}", NAMES[index]),
                    // Two players with the same name.
                    1 => NAMES[(index + 1) % NAMES.len()].to_string(),
                    _ => NAMES[index].to_string(),
                };
                let joins = rng.chance(150).then(|| Moment {
                    round: rng.between(1, 6),
                    during: rng.chance(500),
                });
                let leaves = rng.chance(100).then(|| Moment {
                    round: joins.map_or(0, |joins| joins.round) + rng.between(1, 10),
                    during: rng.chance(500),
                });
                PlayerSpec {
                    account_id: char::from(b'a' + index as u8).to_string(),
                    name,
                    pace: rng.between(950, 1_150),
                    spread: rng.below(1_500),
                    give_up: rng.below(200),
                    joins,
                    leaves,
                    watches: rng.chance(80),
                }
            })
            .collect();

        World {
            mode,
            settings,
            maps,
            playlist,
            players,
            races: Races::Random {
                ties: *rng.pick(&[0, 100, 300]),
                idle: *rng.pick(&[0, 10, 30]),
            },
            faults: Faults {
                repeated_holds: *rng.pick(&[0, 0, 100, 400]),
                forced_rounds: *rng.pick(&[0, 0, 30, 100]),
                missed_starts: *rng.pick(&[0, 0, 20]),
                bridge_restarts: *rng.pick(&[0, 30, 100]),
                script_restarts: *rng.pick(&[0, 0, 0, 10]),
                playlist_changes: *rng.pick(&[0, 0, 20]),
            },
            matches: if rng.chance(200) { 2 } else { 1 },
            max_rounds: 150,
        }
    }

    pub fn map(&self, uid: &str) -> Option<&MapSpec> {
        self.maps.iter().find(|map| map.uid == uid)
    }

    pub fn player(&self, account_id: &str) -> Option<&PlayerSpec> {
        self.players
            .iter()
            .find(|player| player.account_id == account_id)
    }
}

impl PlayerSpec {
    /// A race on a map at the pace of the player.
    pub(crate) fn random_race(&self, map: &MapSpec, rng: &mut Rng, idle: u32) -> RacePlan {
        if rng.chance(idle) {
            return RacePlan::Idle;
        }
        let mut time = 0;
        let checkpoints: Vec<u32> = map
            .sectors
            .iter()
            .map(|&sector| {
                let pace = sector * self.pace / 1000;
                let spread = rng.below(2 * self.spread + 1);
                // At least a second per sector, so checkpoints never come at once.
                time += (pace + spread).saturating_sub(self.spread).max(1_000);
                time
            })
            .collect();
        if rng.chance(self.give_up) {
            let passed = rng.below(checkpoints.len() as u32) as usize;
            RacePlan::GiveUp(checkpoints[..passed].to_vec())
        } else {
            RacePlan::Finish(checkpoints)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worlds_follow_their_seed() {
        let world = |seed| format!("{:?}", World::random(Mode::Knockout, seed));
        assert_eq!(world(5), world(5));
        assert_ne!(world(5), world(6));
    }

    #[test]
    fn players_leave_after_they_joined() {
        for seed in 0..200 {
            for player in World::random(Mode::Rounds, seed).players {
                if let (Some(joins), Some(leaves)) = (player.joins, player.leaves) {
                    assert!(leaves.round > joins.round);
                }
            }
        }
    }

    #[test]
    fn scripted_races_keep_the_proportions_of_the_map() {
        let map = MapSpec {
            uid: "m".into(),
            sectors: vec![10_000, 20_000, 10_000],
        };
        assert_eq!(
            RacePlan::FinishIn(20_000).on(&map),
            RacePlan::Finish(vec![5_000, 15_000, 20_000])
        );
        assert_eq!(
            RacePlan::GiveUpAfter(2).on(&map),
            RacePlan::GiveUp(vec![10_000, 30_000])
        );
        // Giving up never reaches the finish.
        assert_eq!(
            RacePlan::GiveUpAfter(5).on(&map),
            RacePlan::GiveUp(vec![10_000, 30_000])
        );
    }
}
