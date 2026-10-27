//! Helpers for the standings every mode shares.

use std::cmp::{Ordering, Reverse};

use crate::{CanSpawn, Command, Entry, Label, RaceResult, Standings, Status, Tone};

impl Default for Standings {
    /// Standings without players, like the host hands them over when a match starts.
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            round: Vec::new(),
            rounds_on_map: 0,
            rounds_played: 0,
            maps_played: 0,
            info: String::new(),
        }
    }
}

impl Standings {
    /// The entry of a player in the match.
    pub fn player(&self, account_id: &str) -> Option<&Entry> {
        self.entries.iter().find(|p| p.account_id == account_id)
    }

    pub fn player_mut(&mut self, account_id: &str) -> Option<&mut Entry> {
        self.entries.iter_mut().find(|p| p.account_id == account_id)
    }

    /// The entry of a player in the match. A player new to the standings starts
    /// playing without points.
    pub fn join(&mut self, account_id: &str) -> &mut Entry {
        join(&mut self.entries, account_id)
    }

    /// The entry of a player in the round that ends. A player new to the round
    /// starts without points.
    pub fn round_entry(&mut self, account_id: &str) -> &mut Entry {
        join(&mut self.round, account_id)
    }

    /// Players taking part in the match, everyone but spectators.
    pub fn participants(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|p| p.status != Status::Spectator)
    }

    /// Participants who are not eliminated.
    pub fn in_match(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|p| p.is_in_match())
    }

    /// Adds points to the match and to the round of a player.
    pub fn award(&mut self, account_id: &str, points: i32) {
        let player = self.join(account_id);
        player.score = player.score.saturating_add(points);
        let round = self.round_entry(account_id);
        round.score = round.score.saturating_add(points);
    }

    /// Ranks the players the usual way and orders them by position: the players
    /// still in the match by points, players with the same points share a position.
    /// Eliminated players follow and keep the position the mode gave them when they
    /// were eliminated. Spectators come last without a position.
    pub fn rank(&mut self) {
        self.rank_by(|player| Reverse(player.score));
    }

    /// Ranks like [`Standings::rank`], but the players still in the match by a key
    /// instead of their points, the lowest first, e.g. by the time of their last
    /// race. Players with the same key share a position.
    pub fn rank_by<K: Ord>(&mut self, key: impl Fn(&Entry) -> K) {
        fn group(status: Status) -> u8 {
            match status {
                Status::Playing => 0,
                Status::Eliminated => 1,
                Status::Spectator => 2,
            }
        }

        // Stable so ties keep their order.
        self.entries.sort_by(|a, b| {
            group(a.status)
                .cmp(&group(b.status))
                .then_with(|| match a.status {
                    Status::Playing => key(a).cmp(&key(b)),
                    Status::Eliminated => a.position.cmp(&b.position),
                    Status::Spectator => Ordering::Equal,
                })
        });

        let mut previous = None;
        let mut position = 0;
        for (index, player) in self.entries.iter_mut().enumerate() {
            match player.status {
                Status::Playing => {
                    let key = key(player);
                    if previous.as_ref() != Some(&key) {
                        previous = Some(key);
                        position = index as u32 + 1;
                    }
                    player.position = position;
                }
                Status::Eliminated => {}
                Status::Spectator => player.position = 0,
            }
        }
    }

    /// Keeps everyone in the results who does not take part from spawning and shows
    /// them as spectators, e.g. once the participants of a match are known. Nothing
    /// happens without participants. The host leaves out permissions it sent before,
    /// so calling it after every round is fine.
    pub fn block_others(&mut self, results: &[RaceResult]) -> Vec<Command> {
        if self.participants().next().is_none() {
            return Vec::new();
        }
        let mut commands = Vec::new();
        for result in results {
            let status = self.player(&result.account_id).map(|p| p.status);
            if matches!(status, None | Some(Status::Spectator)) {
                self.join(&result.account_id).status = Status::Spectator;
                commands.push(Command::SetCanSpawn(CanSpawn {
                    account_id: result.account_id.clone(),
                    can_spawn: false,
                }));
            }
        }
        commands
    }

    /// Every player ranked first wins the match.
    pub fn crown_leaders(&mut self) {
        for player in self.entries.iter_mut().filter(|p| p.position == 1) {
            player.label = Some(Label::winner());
        }
    }

    /// Places the participants who raced or got points in the round that ends:
    /// finishers by `time`, then everyone else by the checkpoints they passed.
    /// `time` is the race that counts, e.g. the last one in single attempt rounds or
    /// the best one in time attack rounds. The round entries get that time and the
    /// status of the player in the match.
    ///
    /// Call it at the round end once the points are awarded and the players are
    /// ranked, ties keep the order of the standings.
    pub fn place_round(
        &mut self,
        results: &[RaceResult],
        time: impl Fn(&RaceResult) -> Option<u32>,
    ) {
        let mut placed: Vec<(Entry, u32)> = self
            .participants()
            .filter_map(|player| {
                let race = results.iter().find(|r| r.account_id == player.account_id);
                let raced = race.is_some_and(|r| r.started);
                let score = self
                    .round
                    .iter()
                    .find(|r| r.account_id == player.account_id)
                    .map_or(0, |r| r.score);
                (score != 0 || raced).then(|| {
                    let entry = Entry {
                        account_id: player.account_id.clone(),
                        position: 0,
                        score,
                        time: race.and_then(&time),
                        status: player.status,
                        label: None,
                    };
                    (entry, race.map_or(0, |r| r.progress))
                })
            })
            .collect();
        // Stable so ties keep the order of the standings.
        placed.sort_by_key(|(entry, progress)| {
            (entry.time.is_none(), entry.time, Reverse(*progress))
        });

        self.round = placed
            .into_iter()
            .enumerate()
            .map(|(index, (entry, _))| Entry {
                position: index as u32 + 1,
                ..entry
            })
            .collect();
    }
}

fn join<'a>(entries: &'a mut Vec<Entry>, account_id: &str) -> &'a mut Entry {
    let index = match entries.iter().position(|p| p.account_id == account_id) {
        Some(index) => index,
        None => {
            entries.push(Entry {
                account_id: account_id.into(),
                position: 0,
                score: 0,
                time: None,
                status: Status::Playing,
                label: None,
            });
            entries.len() - 1
        }
    };
    &mut entries[index]
}

impl Entry {
    /// Takes part and is not eliminated.
    pub fn is_in_match(&self) -> bool {
        self.status == Status::Playing
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
        for &(account_id, status, score, position) in players {
            let player = standings.join(account_id);
            player.status = status;
            player.score = score;
            player.position = position;
        }
        standings
    }

    fn positions(standings: &Standings) -> Vec<(&str, u32)> {
        standings
            .entries
            .iter()
            .map(|p| (p.account_id.as_str(), p.position))
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
            started: true,
            progress,
            progress_times: vec![],
            previous_time: previous,
            previous_checkpoints: vec![],
            best_time: best,
            best_checkpoints: vec![],
        }
    }

    /// The players of the round in the order of their places.
    fn places(standings: &Standings) -> Vec<(&str, u32, i32)> {
        standings
            .round
            .iter()
            .map(|p| (p.account_id.as_str(), p.position, p.score))
            .collect()
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
            positions(&standings),
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
    fn ranks_by_a_key() {
        let mut standings = standings(&[
            ("eliminated", Status::Eliminated, 0, 5),
            ("no_time", Status::Playing, 0, 0),
            ("slow", Status::Playing, 0, 0),
            ("fast", Status::Playing, 0, 0),
            ("tied", Status::Playing, 0, 0),
        ]);
        for (account_id, time) in [("slow", 31_000), ("fast", 30_000), ("tied", 30_000)] {
            standings.player_mut(account_id).unwrap().time = Some(time);
        }
        standings.rank_by(|p| (p.time.is_none(), p.time));

        assert_eq!(
            positions(&standings),
            [
                ("fast", 1),
                ("tied", 1),
                ("slow", 3),
                ("no_time", 4),
                ("eliminated", 5),
            ]
        );
    }

    #[test]
    fn players_join_once() {
        let mut standings = Standings::default();
        standings.join("a").score = 5;
        assert_eq!(standings.join("a").score, 5);
        assert_eq!(standings.entries.len(), 1);
        assert!(standings.round.is_empty());
    }

    #[test]
    fn award_adds_to_the_match_and_the_round() {
        let mut standings = Standings::default();
        standings.join("a").score = 3;
        standings.award("a", 10);
        standings.award("a", -2);
        assert_eq!(standings.player("a").unwrap().score, 11);
        assert_eq!(standings.round_entry("a").score, 8);
    }

    #[test]
    fn others_cannot_spawn() {
        let mut standings = standings(&[
            ("a", Status::Playing, 0, 1),
            ("out", Status::Eliminated, 0, 2),
        ]);
        let results = [
            race("a", Some(30_000), Some(30_000), 1),
            race("late", Some(29_000), Some(29_000), 1),
            // On the server without racing, e.g. after the elimination.
            RaceResult {
                started: false,
                ..race("out", None, None, 0)
            },
        ];

        assert_eq!(
            standings.block_others(&results),
            [Command::SetCanSpawn(CanSpawn {
                account_id: "late".into(),
                can_spawn: false,
            })]
        );
        assert_eq!(standings.player("late").unwrap().status, Status::Spectator);
        assert_eq!(standings.player("out").unwrap().status, Status::Eliminated);
        // Players can only be told apart from spectators once someone takes part.
        assert!(Standings::default().block_others(&results).is_empty());
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
            standings.entries.iter().map(|p| p.label.clone()).collect();
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
        for (account_id, points) in [("slow", -6), ("fast", -2), ("far", -20), ("left", -20)] {
            standings.round_entry(account_id).score = points;
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
                ("fast", 1, -2),
                ("slow", 2, -6),
                ("far", 3, -20),
                ("spared", 4, 0),
                ("left", 5, -20)
            ]
        );
        assert_eq!(standings.round[0].time, Some(30_000));
        assert_eq!(standings.round[2].time, None);
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
        assert_eq!(places(&standings), [("b", 1, 0), ("a", 2, 0)]);
        standings.place_round(&results, |r| r.previous_time);
        assert_eq!(places(&standings), [("a", 1, 0), ("b", 2, 0)]);
    }
}
