//! Helpers for the standings every mode shares.

use crate::{Standing, Standings, Status};

impl Default for Standings {
    /// Standings without players, like the host hands them over when a match starts.
    fn default() -> Self {
        Self {
            players: Vec::new(),
            rounds_on_map: 0,
            maps_played: 0,
            info: String::new(),
        }
    }
}

impl Standings {
    pub fn player(&self, account_id: &str) -> Option<&Standing> {
        self.players.iter().find(|p| p.account_id == account_id)
    }

    pub fn player_mut(&mut self, account_id: &str) -> Option<&mut Standing> {
        self.players.iter_mut().find(|p| p.account_id == account_id)
    }

    /// The standing of a player. A player new to the standings starts playing
    /// without points. An empty name, e.g. of a player who left, keeps the known one.
    pub fn join(&mut self, account_id: &str, name: &str) -> &mut Standing {
        let index = match self.players.iter().position(|p| p.account_id == account_id) {
            Some(index) => {
                if !name.is_empty() {
                    self.players[index].name = name.into();
                }
                index
            }
            None => {
                self.players.push(Standing {
                    account_id: account_id.into(),
                    name: if name.is_empty() { account_id } else { name }.into(),
                    rank: 0,
                    status: Status::Playing,
                    points: 0,
                    map_points: 0,
                    round_points: 0,
                });
                self.players.len() - 1
            }
        };
        &mut self.players[index]
    }

    /// Players who are not eliminated.
    pub fn in_match(&self) -> impl Iterator<Item = &Standing> {
        self.players
            .iter()
            .filter(|p| p.status != Status::Eliminated)
    }

    /// Ranks the players the usual way and orders them by rank: winners, finalists,
    /// players and players in last chance, each by points. Players with the same
    /// status and points share a rank. Eliminated players come last and keep the rank
    /// the mode gave them when they were eliminated.
    pub fn rank(&mut self) {
        fn group(status: Status) -> u8 {
            match status {
                Status::Winner => 0,
                Status::Finalist => 1,
                Status::Playing => 2,
                Status::LastChance => 3,
                Status::Eliminated => 4,
            }
        }

        // Stable so ties keep their order.
        self.players.sort_by(|a, b| {
            group(a.status)
                .cmp(&group(b.status))
                .then_with(|| match a.status {
                    Status::Eliminated => a.rank.cmp(&b.rank),
                    _ => b.points.cmp(&a.points),
                })
        });

        let mut previous = None;
        let mut rank = 0;
        for (index, player) in self.players.iter_mut().enumerate() {
            if player.status == Status::Eliminated {
                continue;
            }
            let key = (group(player.status), player.points);
            if previous != Some(key) {
                previous = Some(key);
                rank = index as u32 + 1;
            }
            player.rank = rank;
        }
    }

    /// Every player ranked first wins the match.
    pub fn crown_leaders(&mut self) {
        for player in self.players.iter_mut().filter(|p| p.rank == 1) {
            player.status = Status::Winner;
        }
    }
}

impl Standing {
    /// Adds points to the round, the map and the match.
    pub fn award(&mut self, points: i32) {
        self.round_points = self.round_points.saturating_add(points);
        self.map_points = self.map_points.saturating_add(points);
        self.points = self.points.saturating_add(points);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standings(players: &[(&str, Status, i32, u32)]) -> Standings {
        let mut standings = Standings::default();
        for &(account_id, status, points, rank) in players {
            let player = standings.join(account_id, "");
            player.status = status;
            player.points = points;
            player.rank = rank;
        }
        standings
    }

    fn ranks(standings: &Standings) -> Vec<(&str, u32)> {
        standings
            .players
            .iter()
            .map(|p| (p.account_id.as_str(), p.rank))
            .collect()
    }

    #[test]
    fn ranks_by_status_and_points() {
        let mut standings = standings(&[
            ("eliminated_last", Status::Eliminated, 0, 7),
            ("last_chance", Status::LastChance, 0, 0),
            ("third", Status::Playing, 10, 0),
            ("eliminated", Status::Eliminated, 0, 6),
            ("tied_first", Status::Playing, 20, 0),
            ("tied_second", Status::Playing, 20, 0),
            ("winner", Status::Winner, 5, 0),
        ]);
        standings.rank();

        assert_eq!(
            ranks(&standings),
            [
                ("winner", 1),
                ("tied_first", 2),
                ("tied_second", 2),
                ("third", 4),
                ("last_chance", 5),
                ("eliminated", 6),
                ("eliminated_last", 7),
            ]
        );
    }

    #[test]
    fn join_keeps_known_names() {
        let mut standings = Standings::default();
        assert_eq!(standings.join("a", "").name, "a");
        assert_eq!(standings.join("a", "$f00kistz").name, "$f00kistz");
        assert_eq!(standings.join("a", "").name, "$f00kistz");
        assert_eq!(standings.players.len(), 1);
    }

    #[test]
    fn award_adds_to_round_map_and_match() {
        let mut standings = Standings::default();
        let player = standings.join("a", "a");
        player.award(10);
        player.award(-3);
        assert_eq!(
            (player.round_points, player.map_points, player.points),
            (7, 7, 7)
        );
    }

    #[test]
    fn leaders_win() {
        let mut standings = standings(&[
            ("a", Status::Playing, 10, 0),
            ("b", Status::Playing, 10, 0),
            ("c", Status::Playing, 5, 0),
        ]);
        standings.rank();
        standings.crown_leaders();
        let statuses: Vec<Status> = standings.players.iter().map(|p| p.status).collect();
        assert_eq!(statuses, [Status::Winner, Status::Winner, Status::Playing]);
    }
}
