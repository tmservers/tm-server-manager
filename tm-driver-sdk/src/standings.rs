//! Helpers for the standings every mode shares.

use std::cmp::{Ordering, Reverse};

use crate::{Label, RaceResult, Standing, Standings, Status, Tone};

impl Default for Standings {
    /// Standings without players, like the host hands them over when a match starts.
    fn default() -> Self {
        Self {
            players: Vec::new(),
            rounds_on_map: 0,
            maps_played: 0,
            map_name: String::new(),
            map_count: 0,
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
                    label: None,
                    points: 0,
                    map_points: 0,
                    round_points: 0,
                    round_position: 0,
                });
                self.players.len() - 1
            }
        };
        &mut self.players[index]
    }

    /// Players taking part in the match, everyone but spectators.
    pub fn participants(&self) -> impl Iterator<Item = &Standing> {
        self.players
            .iter()
            .filter(|p| p.status != Status::Spectator)
    }

    /// Participants who are not eliminated.
    pub fn in_match(&self) -> impl Iterator<Item = &Standing> {
        self.players.iter().filter(|p| p.is_in_match())
    }

    /// Ranks the players the usual way and orders them by rank: the players still
    /// in the match by points, players with the same points share a rank. Eliminated
    /// players follow and keep the rank the mode gave them when they were eliminated.
    /// Spectators come last unranked.
    pub fn rank(&mut self) {
        fn group(status: Status) -> u8 {
            match status {
                Status::Playing => 0,
                Status::Eliminated => 1,
                Status::Spectator => 2,
            }
        }

        // Stable so ties keep their order.
        self.players.sort_by(|a, b| {
            group(a.status)
                .cmp(&group(b.status))
                .then_with(|| match a.status {
                    Status::Playing => b.points.cmp(&a.points),
                    Status::Eliminated => a.rank.cmp(&b.rank),
                    Status::Spectator => Ordering::Equal,
                })
        });

        let mut previous = None;
        let mut rank = 0;
        for (index, player) in self.players.iter_mut().enumerate() {
            match player.status {
                Status::Playing => {
                    if previous != Some(player.points) {
                        previous = Some(player.points);
                        rank = index as u32 + 1;
                    }
                    player.rank = rank;
                }
                Status::Eliminated => {}
                Status::Spectator => player.rank = 0,
            }
        }
    }

    /// Every player ranked first wins the match.
    pub fn crown_leaders(&mut self) {
        for player in self.players.iter_mut().filter(|p| p.rank == 1) {
            player.label = Some(Label::winner());
        }
    }

    /// Places the participants in the round who raced or got points: finishers by
    /// `time`, then everyone else by the checkpoints they passed. `time` is the race
    /// that counts, e.g. the last one in single attempt rounds or the best one in time
    /// attack rounds. Call it at the round end once the round points are awarded and
    /// the players are ranked, ties keep the order of the standings.
    pub fn place_round(
        &mut self,
        results: &[RaceResult],
        time: impl Fn(&RaceResult) -> Option<u32>,
    ) {
        let mut placed: Vec<(usize, Option<u32>, u32)> =
            self.players
                .iter()
                .enumerate()
                .filter_map(|(index, player)| {
                    let race = results.iter().find(|r| r.account_id == player.account_id);
                    let raced = race.is_some_and(|r| r.started || time(r).is_some());
                    (player.status != Status::Spectator && (player.round_points != 0 || raced))
                        .then(|| {
                            (
                                index,
                                race.and_then(|r| time(r)),
                                race.map_or(0, |r| r.progress),
                            )
                        })
                })
                .collect();
        // Stable so ties keep the order of the standings.
        placed.sort_by_key(|&(_, time, progress)| (time.is_none(), time, Reverse(progress)));

        for player in &mut self.players {
            player.round_position = 0;
        }
        for (position, (index, _, _)) in placed.into_iter().enumerate() {
            self.players[index].round_position = position as u32 + 1;
        }
    }
}

impl Standing {
    /// Takes part and is not eliminated.
    pub fn is_in_match(&self) -> bool {
        self.status == Status::Playing
    }

    /// Adds points to the round, the map and the match.
    pub fn award(&mut self, points: i32) {
        self.round_points = self.round_points.saturating_add(points);
        self.map_points = self.map_points.saturating_add(points);
        self.points = self.points.saturating_add(points);
    }
}

impl Label {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }

    /// Players who won the match.
    pub fn winner() -> Self {
        Self::new("Winner", Tone::Positive)
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

    fn race(
        account_id: &str,
        previous: Option<u32>,
        best: Option<u32>,
        progress: u32,
    ) -> RaceResult {
        RaceResult {
            account_id: account_id.into(),
            name: account_id.into(),
            started: true,
            progress,
            previous_time: previous,
            previous_checkpoints: vec![],
            best_time: best,
            best_checkpoints: vec![],
        }
    }

    /// The players in the order of their places in the round, unplaced ones left out.
    fn places(standings: &Standings) -> Vec<(&str, u32)> {
        let mut places: Vec<(&str, u32)> = standings
            .players
            .iter()
            .filter(|p| p.round_position > 0)
            .map(|p| (p.account_id.as_str(), p.round_position))
            .collect();
        places.sort_by_key(|&(_, position)| position);
        places
    }

    #[test]
    fn ranks_by_status_and_points() {
        let mut standings = standings(&[
            ("spectator", Status::Spectator, 0, 3),
            ("eliminated_last", Status::Eliminated, 0, 7),
            ("no_points", Status::Playing, 0, 0),
            ("third", Status::Playing, 10, 0),
            ("eliminated", Status::Eliminated, 0, 6),
            ("tied_first", Status::Playing, 20, 0),
            ("tied_second", Status::Playing, 20, 0),
        ]);
        standings.rank();

        assert_eq!(
            ranks(&standings),
            [
                ("tied_first", 1),
                ("tied_second", 1),
                ("third", 3),
                ("no_points", 4),
                ("eliminated", 6),
                ("eliminated_last", 7),
                ("spectator", 0),
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
        let labels: Vec<Option<Label>> =
            standings.players.iter().map(|p| p.label.clone()).collect();
        assert_eq!(labels, [Some(Label::winner()), Some(Label::winner()), None]);
    }

    #[test]
    fn rounds_are_placed_by_the_race() {
        let mut standings = standings(&[
            // Lost nothing without a finish, still behind every finisher.
            ("spared", Status::Playing, 0, 1),
            ("slow", Status::Playing, 0, 2),
            ("fast", Status::Playing, 0, 3),
            ("far", Status::Playing, 0, 4),
            // Left before starting but lost points.
            ("left", Status::Playing, 0, 5),
            // Did not race and got nothing.
            ("idle", Status::Playing, 0, 6),
            ("spectator", Status::Spectator, 0, 0),
        ]);
        for (account_id, round_points) in [("slow", -6), ("fast", -2), ("far", -20), ("left", -20)]
        {
            standings.player_mut(account_id).unwrap().round_points = round_points;
        }
        let results = [
            race("slow", Some(31_000), Some(31_000), 3),
            race("spared", None, None, 1),
            race("far", None, None, 2),
            race("fast", Some(30_000), Some(30_000), 3),
            // Not in the standings, e.g. a late player.
            race("late", Some(29_000), Some(29_000), 3),
            // Raced without taking part.
            race("spectator", Some(28_000), Some(28_000), 3),
        ];

        standings.place_round(&results, |r| r.previous_time);
        assert_eq!(
            places(&standings),
            [
                ("fast", 1),
                ("slow", 2),
                ("far", 3),
                ("spared", 4),
                ("left", 5)
            ]
        );
    }

    #[test]
    fn rounds_are_placed_by_the_race_that_counts() {
        let mut standings =
            standings(&[("a", Status::Playing, 0, 0), ("b", Status::Playing, 0, 0)]);
        let results = [
            race("a", Some(42_000), Some(42_000), 3),
            race("b", Some(45_000), Some(41_000), 3),
        ];

        standings.place_round(&results, |r| r.best_time);
        assert_eq!(places(&standings), [("b", 1), ("a", 2)]);
        standings.place_round(&results, |r| r.previous_time);
        assert_eq!(places(&standings), [("a", 1), ("b", 2)]);
    }
}
