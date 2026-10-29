//! What surrounds the driver in a simulation: the server with its maps and
//! players, how they race and what goes wrong, all random from a seed.

use crate::Rng;

/// A server holds at most this many players.
pub const MAX_PLAYERS: u32 = 120;

#[derive(Debug, Clone)]
pub struct World {
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
    /// New settings an admin gives the match in this order, see
    /// [`Faults::settings_changes`].
    pub settings_changes: Vec<SettingsChange>,
}

/// New settings during a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsChange {
    pub settings: String,
    /// Only what can change during a match changes, so the mode takes them.
    pub reloadable: bool,
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

/// How players race, beyond the pace of every player.
#[derive(Debug, Clone, Copy)]
pub struct Races {
    /// Chance of a round with two equal races in per mille.
    pub ties: u32,
    /// Chance that a player does not move in a race in per mille.
    pub idle: u32,
}

/// A race as it is going to happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RacePlan {
    /// Reaches the finish with these race times at the checkpoints, the finish last.
    Finish(Vec<u32>),
    /// Gives up after passing checkpoints at these race times.
    GiveUp(Vec<u32>),
    /// Spawns but does not move.
    Idle,
}

/// Something that happens before a round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Join(String),
    Leave(String),
    /// The modescript restarts, which starts a new match.
    RestartScript,
    /// The driver gets a new playlist.
    Playlist(Vec<String>),
    /// An admin gives the match the next new settings of the world.
    ChangeSettings,
}

/// How often things go wrong, all in per mille.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// Rounds an admin pauses.
    pub pauses: u32,
    /// Rounds before or during which an admin changes the settings.
    pub settings_changes: u32,
}

const NAMES: [&str; 16] = [
    "Ant", "Bea", "Cid", "Dot", "Eve", "Fay", "Gus", "Hal", "Ivy", "Jo", "Kai", "Lu", "Max", "Ned",
    "Oli", "Pia",
];

impl World {
    /// A random world with the settings of the mode. Everything else varies with the
    /// seed: the maps, how many players there are, how they race, when they come and
    /// go and what goes wrong. Most worlds are plain, some have every fault and a few
    /// a crowd of players.
    pub fn random(seed: u64, settings: String) -> World {
        Self::generate(seed, settings, None)
    }

    /// A random world on a full server.
    pub fn full_server(seed: u64, settings: String) -> World {
        Self::generate(seed, settings, Some(MAX_PLAYERS))
    }

    /// Every part draws from its own stream, so a change to one does not reshuffle
    /// the others.
    fn generate(seed: u64, settings: String, players: Option<u32>) -> World {
        let rng = Rng::new(seed).fork("world");

        let mut maps = rng.fork("maps");
        let maps: Vec<MapSpec> = (1..=maps.between(1, 3))
            .map(|n| MapSpec {
                uid: format!("m{n}"),
                sectors: (0..maps.between(2, 5))
                    .map(|_| maps.between(3_000, 12_000))
                    .collect(),
            })
            .collect();

        let mut order = rng.fork("playlist");
        let mut playlist: Vec<String> = maps.iter().map(|map| map.uid.clone()).collect();
        match order.below(4) {
            0 => playlist.clear(),
            1 => {
                order.shuffle(&mut playlist);
                playlist.truncate(order.between(1, playlist.len() as u32) as usize);
            }
            _ => {}
        }

        let count = players.unwrap_or_else(|| {
            let mut crowd = rng.fork("crowd");
            match crowd.below(100) {
                0..=1 => 0,
                2..=6 => 1,
                7..=36 => crowd.between(2, 4),
                37..=76 => crowd.between(5, 8),
                77..=95 => crowd.between(9, 16),
                _ => crowd.between(17, MAX_PLAYERS),
            }
        });
        let players: Vec<PlayerSpec> = (0..count as usize)
            .map(|index| PlayerSpec::random(index, &mut rng.fork(&format!("player/{index}"))))
            .collect();

        let mut chances = rng.fork("chances");
        let races = Races {
            ties: *chances.pick(&[0, 100, 300]),
            idle: *chances.pick(&[0, 10, 30]),
        };
        let mut faults = Faults {
            repeated_holds: *chances.pick(&[0, 0, 100, 400]),
            forced_rounds: *chances.pick(&[0, 0, 30, 100]),
            missed_starts: *chances.pick(&[0, 0, 20]),
            bridge_restarts: *chances.pick(&[0, 30, 100]),
            script_restarts: *chances.pick(&[0, 0, 0, 10]),
            playlist_changes: *chances.pick(&[0, 0, 20]),
            pauses: *rng.fork("pauses").pick(&[0, 0, 20, 60]),
            settings_changes: *rng.fork("settings changes").pick(&[0, 0, 30, 100]),
        };
        // Plain matches show the rules of a mode without noise, and failures in them
        // are the easiest to read. A stream of its own leaves the rest of the world
        // as it is.
        if rng.fork("plain").chance(400) {
            faults = Faults::default();
        }
        World {
            settings,
            maps,
            playlist,
            races,
            faults,
            matches: if chances.chance(200) { 2 } else { 1 },
            // Crowded matches take long, the rounds tell enough.
            max_rounds: if players.len() > 16 { 40 } else { 150 },
            players,
            // The simulator plans them, it knows the settings of the mode.
            settings_changes: Vec::new(),
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
    /// A player of a random world with its own pace, maybe late, maybe leaving early
    /// or only watching.
    fn random(index: usize, rng: &mut Rng) -> PlayerSpec {
        let name = match index / NAMES.len() {
            0 => NAMES[index].to_string(),
            again => format!("{}{again}", NAMES[index % NAMES.len()]),
        };
        let name = match rng.below(20) {
            0 => format!("$f00{name}"),
            // The same name as someone else.
            1 => NAMES[(index + 1) % NAMES.len()].to_string(),
            _ => name,
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
            account_id: account_id(index),
            name,
            pace: rng.between(950, 1_150),
            spread: rng.below(1_500),
            give_up: rng.below(200),
            joins,
            leaves,
            watches: rng.chance(80),
        }
    }

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

/// Short accounts in the order of the players: `a` to `z`, then `aa`, `ab` and so on.
fn account_id(index: usize) -> String {
    let mut letters = Vec::new();
    let mut rest = index + 1;
    while rest > 0 {
        rest -= 1;
        letters.push(b'a' + (rest % 26) as u8);
        rest /= 26;
    }
    letters.into_iter().rev().map(char::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worlds_follow_their_seed() {
        let world = |seed| format!("{:?}", World::random(seed, String::new()));
        assert_eq!(world(5), world(5));
        assert_ne!(world(5), world(6));
    }

    #[test]
    fn many_worlds_are_plain_and_some_have_every_fault() {
        let worlds: Vec<Faults> = (0..1000)
            .map(|seed| World::random(seed, String::new()).faults)
            .collect();
        let plain = worlds
            .iter()
            .filter(|faults| **faults == Faults::default())
            .count();
        let everything = worlds
            .iter()
            .filter(|faults| {
                [
                    faults.repeated_holds,
                    faults.forced_rounds,
                    faults.missed_starts,
                    faults.bridge_restarts,
                    faults.script_restarts,
                    faults.playlist_changes,
                    faults.pauses,
                    faults.settings_changes,
                ]
                .iter()
                .all(|&chance| chance > 0)
            })
            .count();
        assert!((350..500).contains(&plain), "{plain} plain worlds");
        assert!(everything > 0);
    }

    #[test]
    fn players_leave_after_they_joined() {
        for seed in 0..200 {
            for player in World::random(seed, String::new()).players {
                if let (Some(joins), Some(leaves)) = (player.joins, player.leaves) {
                    assert!(leaves.round > joins.round);
                }
            }
        }
    }

    #[test]
    fn some_worlds_are_crowded() {
        let counts: Vec<usize> = (0..300)
            .map(|seed| World::random(seed, String::new()).players.len())
            .collect();
        assert!(counts.iter().any(|&count| count > 16));
        assert!(counts.iter().all(|&count| count <= MAX_PLAYERS as usize));

        let full = World::full_server(1, String::new());
        assert_eq!(full.players.len(), MAX_PLAYERS as usize);
        assert_eq!(full.players[119].account_id, "dp");
        let accounts: std::collections::BTreeSet<&str> = full
            .players
            .iter()
            .map(|player| player.account_id.as_str())
            .collect();
        assert_eq!(accounts.len(), full.players.len());
    }

    #[test]
    fn accounts_go_on_after_z() {
        let accounts: Vec<String> = [0, 25, 26, 27, 51, 52, 701, 702]
            .into_iter()
            .map(account_id)
            .collect();
        assert_eq!(accounts, ["a", "z", "aa", "ab", "az", "ba", "zz", "aaa"]);
    }
}
