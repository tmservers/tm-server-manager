//! Knockout mode for the wasm driver.
//!
//! Follows the rules of the community Multi Lives Knockout mode, which gives the
//! players of the Nadeo Knockout mode lives: the players race rounds and the last
//! players of every round lose a life until one player is left, who wins. A
//! player who loses their last life is eliminated. Everyone without a finish
//! loses a life as well, even beyond the losses of the round. The more players
//! are left, the more lose a life per round. With one life, the default, this is
//! the Nadeo Knockout mode.
//!
//! The first rounds of the match take no lives. The players who start a race
//! before the first round that takes lives ended take part, everyone else cannot
//! spawn anymore.
//!
//! Participants, their lives and statuses live in the standings the host keeps:
//! the points of a player are the lives they have left. Players still in the
//! match are ranked by their lives and their last race, eliminated players keep
//! the rank they were eliminated with. Nobody shares a rank: equal races go to the
//! better times at the checkpoints and finally by account.

use serde::{Deserialize, Serialize};
use tm_driver_sdk::{
    CanSpawn, Command, CountedRace, Entry, Event, Label, Mode, RoundEnd, RoundFormat, Standings,
    Status, Tone,
    settings::{FinishTimeoutSetting, WarmUpSetting},
};

tm_driver_sdk::export_mode!(Knockout);

/// Icons of the game font for a life left and a lost life.
const LIFE: &str = "\u{e96c}";
const LOST_LIFE: &str = "\u{e96f}";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Lives every participant starts with, at least 1. Players who lose their last
    /// life are eliminated, so with 1 life this is the original Knockout.
    pub lives: i32,
    /// Rounds at the start of the match in which nobody loses a life.
    pub rounds_without_elimination: u32,
    /// One more player loses a life per round for every milestone still ahead.
    pub extra_losses: ExtraLosses,
    /// Valid rounds played on every map. 0 means no limit.
    pub rounds_per_map: u32,
    pub finish_timeout: FinishTimeoutSetting,
    /// The warmup before the first round of every map.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lives: 1,
            rounds_without_elimination: 1,
            extra_losses: ExtraLosses::AbovePlayersLeft(vec![4, 16, 16]),
            rounds_per_map: 0,
            finish_timeout: FinishTimeoutSetting::Milliseconds(5_000),
            warmup: WarmUpSetting::default(),
        }
    }
}

/// The milestones of `extra_losses`. They count in any order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraLosses {
    /// Numbers of players left: `[4, 16, 16]` takes a life from 4 players per round
    /// above 16 players, from 2 above 4 players and from 1 from there on.
    AbovePlayersLeft(Vec<u32>),
    /// Numbers of lives the players left have together.
    AboveLivesLeft(Vec<u32>),
    /// Rounds of the match: `[8, 16]` takes a life from 3 players per round before
    /// round 8, from 2 before round 16 and from 1 from there on.
    BeforeRound(Vec<u32>),
}

#[derive(Debug, Clone)]
pub struct Knockout {
    settings: Settings,
}

impl Mode for Knockout {
    const NAME: &'static str = "Knockout";
    /// Everything but the lives, which players already hold. More rounds without
    /// elimination open the registration again.
    const RELOADABLE: &'static [&'static str] = &[
        "rounds_without_elimination",
        "extra_losses",
        "rounds_per_map",
        "finish_timeout",
        "warmup",
    ];

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        if settings.lives < 1 {
            return Err("Invalid settings: lives must be at least 1".into());
        }
        Ok(Self { settings })
    }

    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        let commands = self.handle(standings, event);
        // The labels and the info follow from the standings alone.
        self.label(standings);
        standings.info = self.info(standings);
        commands
    }

    /// The race so far. Who loses a life is only known once the round ended, also
    /// players without a finish do.
    fn live_round(&self, standings: &Standings, races: &RoundEnd) -> Vec<Entry> {
        standings.placed(races, CountedRace::Last, |_| 0)
    }
}

impl Knockout {
    fn handle(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active. New settings send them
            // again, the round format applies from the next round on.
            Event::MapStart | Event::SettingsChanged => vec![
                Command::SetRoundFormat(RoundFormat::SingleAttempt(
                    self.settings.finish_timeout.into(),
                )),
                Command::SetWarmUp(self.settings.warmup.into()),
            ],
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            Event::MapEnd => {
                if self.match_over(standings) {
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            // Players take part with all their lives from their first race on, so the
            // standings show them during the first round.
            Event::RaceStart(account_id) => {
                if self.registration_open(standings) && standings.player(&account_id).is_none() {
                    standings.join(&account_id).score = self.settings.lives;
                }
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
    }

    /// With one life the time of the last round for the players in the match like
    /// in the scores table of the original Knockout, with more lives the lives they
    /// have left like in the scores table of the multi lives script. `K.O.` for
    /// eliminated players and `Winner` once the match is over. The labels follow
    /// from the standings alone.
    fn label(&self, standings: &mut Standings) {
        let match_over = self.match_over(standings);
        for player in &mut standings.entries {
            player.label = match player.status {
                Status::Playing if match_over => Some(Label::winner()),
                Status::Playing if self.settings.lives == 1 => player
                    .time
                    .map(|time| Label::new(race_time(time), Tone::Neutral)),
                Status::Playing => Some(self.lives_label(player.score)),
                Status::Eliminated => Some(Label::new("K.O.", Tone::Negative)),
                Status::Spectator => None,
            };
        }
    }

    /// Hearts for the lost lives and the lives left, as text when there would be
    /// too many hearts. The last life is in danger.
    fn lives_label(&self, left: i32) -> Label {
        let text = if self.settings.lives > 5 {
            match left {
                1 => "1 life".to_string(),
                left => format!("{left} lives"),
            }
        } else {
            let lost = usize::try_from(self.settings.lives - left).unwrap_or(0);
            LOST_LIFE.repeat(lost) + &LIFE.repeat(usize::try_from(left).unwrap_or(0))
        };
        let tone = if left > 1 {
            Tone::Neutral
        } else {
            Tone::Negative
        };
        Label::new(text, tone)
    }

    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // The host counted the round unless it is invalid.
        let round = standings.rounds_played + u32::from(round_end.invalid);
        // Players can join until the first round that takes lives ended. Their race
        // starts brought them in already, unless e.g. the driver restarted.
        if (round <= self.settings.rounds_without_elimination + 1 && !self.lives_taken(standings))
            || standings.participants().next().is_none()
        {
            self.register(standings, &round_end);
        }
        if round_end.invalid || standings.participants().next().is_none() {
            return self.block_late_players(standings, &round_end);
        }

        let mut commands = self.take_lives(standings, round, &round_end);
        standings.rounds_on_map += 1;
        commands.extend(self.block_late_players(standings, &round_end));
        // The points are the lives.
        standings.rank(&round_end, CountedRace::Last);
        standings.place_round(&round_end, CountedRace::Last);

        if self.match_over(standings) {
            commands.extend(winner_message(standings).map(Command::ChatMessage));
            commands.push(Command::EndMatch);
        } else if self.settings.rounds_per_map > 0
            && standings.rounds_on_map >= self.settings.rounds_per_map
        {
            commands.push(Command::EndMap);
        }
        commands
    }

    /// Takes a life from the last players of the race and everyone without a
    /// finish. Players who lose their last life are eliminated with the last ranks
    /// of the players left, in the order of the race. Everyone in the match gets the
    /// time of the round.
    fn take_lives(
        &self,
        standings: &mut Standings,
        round: u32,
        round_end: &RoundEnd,
    ) -> Vec<Command> {
        let order = tm_driver_sdk::race_order(
            standings.in_match().map(|p| p.account_id.as_str()),
            round_end,
            CountedRace::Last,
        );
        let time = |account_id: &str| {
            round_end
                .racer(account_id)
                .and_then(|racer| CountedRace::Last.time(racer))
        };
        let without_finish = order
            .iter()
            .filter(|account_id| time(account_id).is_none())
            .count();
        let losses = match self.losses(standings, round) {
            0 => 0,
            // Everyone without a finish loses a life, the slowest finishers fill up
            // the losses of the round.
            losses => losses.max(without_finish),
        };
        let last_life = |account_id: &str| {
            standings
                .player(account_id)
                .is_some_and(|player| player.score <= 1)
        };
        let mut first_loser = order.len() - losses;
        // Rather than everyone being out, the player who got the furthest keeps
        // their last life and wins, even without a finish.
        if first_loser == 0 && order.iter().all(|account_id| last_life(account_id)) {
            first_loser = 1;
        }
        let eliminated = order
            .iter()
            .skip(first_loser)
            .filter(|account_id| last_life(account_id))
            .count();
        let mut position = (order.len() - eliminated) as u32;

        let mut commands = Vec::new();
        for (index, account_id) in order.iter().enumerate() {
            let loses_life = index >= first_loser;
            if loses_life {
                // The round shows who lost a life.
                standings.round_entry(account_id).score = -1;
            }
            let player = standings
                .player_mut(account_id)
                .expect("Only players in the match raced");
            player.time = time(account_id);
            if !loses_life {
                continue;
            }
            player.score -= 1;
            if player.score > 0 {
                continue;
            }
            position += 1;
            player.status = Status::Eliminated;
            player.position = position;
            commands.push(Command::ChatMessage(format!(
                "$f90$i$<{}$> takes {} place!",
                tm_driver_sdk::name(account_id),
                tm_driver_sdk::ordinal(position)
            )));
            commands.push(Command::SetCanSpawn(CanSpawn {
                account_id: account_id.clone(),
                can_spawn: false,
            }));
        }
        commands
    }

    /// Everyone who started a race takes part with all their lives while players
    /// can join.
    fn register(&self, standings: &mut Standings, round_end: &RoundEnd) {
        for racer in &round_end.racers {
            if standings.player(&racer.account_id).is_none() {
                standings.join(&racer.account_id).score = self.settings.lives;
            }
        }
    }

    /// Players who could still take part are not blocked.
    fn block_late_players(&self, standings: &mut Standings, round_end: &RoundEnd) -> Vec<Command> {
        if self.registration_open(standings) {
            Vec::new()
        } else {
            standings.block_others(round_end)
        }
    }

    /// Players can join until the first round that takes lives ended, and as long
    /// as nobody takes part. Once lives were taken, it stays closed, also when more
    /// rounds without elimination are set during the match.
    fn registration_open(&self, standings: &Standings) -> bool {
        (standings.rounds_played <= self.settings.rounds_without_elimination
            && !self.lives_taken(standings))
            || standings.participants().next().is_none()
    }

    /// A round took lives in this match. The lives can not change during a match.
    fn lives_taken(&self, standings: &Standings) -> bool {
        standings
            .participants()
            .any(|p| p.status == Status::Eliminated || p.score < self.settings.lives)
    }

    /// One player is left and nobody can join anymore.
    fn match_over(&self, standings: &Standings) -> bool {
        !self.registration_open(standings) && standings.in_match().count() <= 1
    }

    /// The players who lose a life in a round of the match as it stands. Players
    /// without a finish can add to them.
    fn losses(&self, standings: &Standings, round: u32) -> usize {
        let lives_left = standings
            .in_match()
            .map(|player| usize::try_from(player.score).unwrap_or(0))
            .sum();
        self.round_losses(round, standings.in_match().count(), lives_left)
    }

    /// One player per round once the rounds without elimination are over, plus one
    /// for every milestone of `extra_losses` still ahead. One player always stays.
    fn round_losses(&self, round: u32, players_left: usize, lives_left: usize) -> usize {
        if round <= self.settings.rounds_without_elimination {
            return 0;
        }
        let extra = match &self.settings.extra_losses {
            ExtraLosses::AbovePlayersLeft(milestones) => milestones
                .iter()
                .filter(|&&above| (above as usize) < players_left)
                .count(),
            ExtraLosses::AboveLivesLeft(milestones) => milestones
                .iter()
                .filter(|&&above| (above as usize) < lives_left)
                .count(),
            ExtraLosses::BeforeRound(milestones) => {
                milestones.iter().filter(|&&before| before > round).count()
            }
        };
        (1 + extra).min(players_left.saturating_sub(1))
    }

    /// The estimated rounds of a match with one life from the round that runs or
    /// comes next on, unless players without a finish speed it up.
    fn rounds(&self, round: u32, mut left: usize) -> u32 {
        // The eliminations start with the next round that has them and go on until
        // one player is left.
        let mut last = round.max(self.settings.rounds_without_elimination + 1);
        while left > 1 {
            left -= self.round_losses(last, left, left);
            if left > 1 {
                last += 1;
            }
        }
        last
    }

    /// The round that runs or comes next with the players who lose a life in it,
    /// e.g. `Round 2 / 22, 64 left, 4 K.O.` with one life. Lives make the rounds of
    /// the match unpredictable, so with more lives e.g. `Round 2, 64 left, 4 lose a
    /// life`.
    fn info(&self, standings: &Standings) -> String {
        if self.match_over(standings) {
            return "Match over".into();
        }
        let round = standings.rounds_played + 1;
        let left = standings.in_match().count();
        let losses = self.losses(standings, round);
        let (rounds, losses) = if self.settings.lives == 1 {
            let losses = match losses {
                0 => "no K.O.".to_string(),
                losses => format!("{losses} K.O."),
            };
            (
                format!("Round {round} / {}", self.rounds(round, left)),
                losses,
            )
        } else {
            let losses = match losses {
                0 => "no loss of life".to_string(),
                1 => "1 loses a life".to_string(),
                losses => format!("{losses} lose a life"),
            };
            (format!("Round {round}"), losses)
        };
        if standings.participants().next().is_none() {
            format!("{rounds}, {losses}")
        } else {
            format!("{rounds}, {left} left, {losses}")
        }
    }
}

/// The last player in the match wins.
fn winner_message(standings: &Standings) -> Option<String> {
    standings.in_match().next().map(|winner| {
        format!(
            "$<{}$> wins the match!",
            tm_driver_sdk::name(&winner.account_id)
        )
    })
}

/// A race time the way the game shows it, e.g. `1:02.345`.
fn race_time(milliseconds: u32) -> String {
    format!(
        "{}:{:02}.{:03}",
        milliseconds / 60_000,
        milliseconds / 1_000 % 60,
        milliseconds % 1_000
    )
}

#[cfg(test)]
mod tests;
