//! TMWT 2v2 for the mode driver, version 1.
//!
//! Follows the rules of Nadeo's script of the Trackmania World Tour: two teams of two
//! race rounds. The order in which the teams finish decides the points of a round:
//!
//! | Finishers by team | Result | Points |
//! |---|---|---|
//! | A A B B, A A B, A A, A | Ace | 3 – 0 |
//! | A B A B, A B A, A B | Victory | 2 – 1 |
//! | A B B A, A B B | Draw | 1 – 1 |
//! | nobody | | 0 – 0 |
//!
//! A team wins the map once it has at least `map_points_limit` points and more than
//! the other team, while both are level at the limit the map goes on in overtime. The
//! team which won the map gets a match point, the first team with `match_points_limit`
//! match points wins the match. Rounds which do not count, e.g. forced ones, give no
//! points. Players never give up, they respawn at their last checkpoint.
//!
//! Places come from the races like in every mode, so nobody ties. Players get the
//! points of their team in the rounds they finished. Teams rank by match points, then
//! by the points on the map, finally by team.

use serde::{Deserialize, Serialize};
use tm_mode_driver_sdk::{
    Command, CountedRace, Entry, Event, Host, Label, Laps, Mode, RaceRules, Respawn, RoundEnd,
    RoundFormat, Standings, TeamEntry, Tone, race_order, settings::FinishTimeoutSetting,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Points a team needs on a map to win it, with more points than the other team.
    pub map_points_limit: i32,
    /// Maps a team needs to win the match, e.g. 4 for a best of seven.
    pub match_points_limit: i32,
    pub finish_timeout: FinishTimeoutSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            map_points_limit: 10,
            match_points_limit: 4,
            finish_timeout: FinishTimeoutSetting::Automatic,
        }
    }
}

/// How the teams finished a round, the team which finished first is `A`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nobody finished.
    None,
    /// The first two finishers are of the first team, or only one player finished.
    Ace,
    /// The first team has the first and the third finisher, or the other team only the second.
    Victory,
    /// The other team has the second and the third finisher.
    Draw,
}

impl Outcome {
    /// The outcome of the teams of the finishers in the order they finished.
    pub fn of(finishers: &[u32]) -> Outcome {
        match finishers {
            [] => Outcome::None,
            [_] => Outcome::Ace,
            [a, b, ..] if a == b => Outcome::Ace,
            [_, _] => Outcome::Victory,
            [a, _, c, ..] if a == c => Outcome::Victory,
            _ => Outcome::Draw,
        }
    }

    /// What the chat tells about the round, the team which finished first by its name.
    pub fn announcement(self, leader: &str) -> Option<String> {
        match self {
            Outcome::None => None,
            Outcome::Ace => Some(format!("Ace for {leader}")),
            Outcome::Victory => Some(format!("Victory for {leader}")),
            Outcome::Draw => Some("Draw".into()),
        }
    }

    /// The points of the first team and of the other team.
    pub fn points(self) -> (i32, i32) {
        match self {
            Outcome::None => (0, 0),
            Outcome::Ace => (3, 0),
            Outcome::Victory => (2, 1),
            Outcome::Draw => (1, 1),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tmwt2v2 {
    settings: Settings,
}

impl Mode for Tmwt2v2 {
    const NAME: &'static str = "TMWT 2v2";
    /// The limits apply from the next round on.
    const RELOADABLE: &'static [&'static str] =
        &["map_points_limit", "match_points_limit", "finish_timeout"];

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        if settings.map_points_limit < 1 || settings.match_points_limit < 1 {
            return Err("Teams need at least one point to win a map and the match.".into());
        }
        Ok(Self { settings })
    }

    fn step(&self, host: &dyn Host, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            Event::MatchStart => {
                join_teams(host, standings);
                rank_teams(standings);
                standings.info = self.info(standings);
                Vec::new()
            }
            Event::MapStart | Event::SettingsChanged => {
                if let Event::MapStart = event {
                    for team in &mut standings.teams {
                        team.map_score = 0;
                    }
                }
                join_teams(host, standings);
                self.label_teams(standings);
                rank_teams(standings);
                standings.info = self.info(standings);
                vec![
                    Command::SetRoundFormat(RoundFormat::SingleAttempt(
                        self.settings.finish_timeout.into(),
                    )),
                    // Nobody gives up, a player respawns at the last checkpoint.
                    Command::SetRaceRules(RaceRules {
                        laps: Laps::Map,
                        respawn: Some(Respawn::NeverGiveUp),
                    }),
                ]
            }
            Event::RaceStart(account_id) => {
                standings.join(&account_id);
                Vec::new()
            }
            Event::RoundEnd(round_end) => self.round_end(host, standings, round_end),
            Event::RoundStart | Event::MapEnd | Event::MatchEnd => Vec::new(),
            // Its manialinks send nothing to the mode.
            Event::UiAction(_) => Vec::new(),
        }
    }

    /// The finishers so far with the points of their team as if the round ended now.
    fn live_round(&self, host: &dyn Host, standings: &Standings, races: &RoundEnd) -> Vec<Entry> {
        let races = races.taking_part(standings);
        let points = round_points(host, &races);
        standings.placed(&races, CountedRace::Last, |account_id| {
            player_points(host, &points, &races, account_id)
        })
    }
}

/// The points of the teams in a round: the outcome, the team which finished first and
/// every team with its points.
struct RoundPoints {
    outcome: Outcome,
    leader: Option<u32>,
    teams: Vec<(u32, i32)>,
}

/// The finishers of the round in race order.
fn finishers(round_end: &RoundEnd) -> Vec<String> {
    race_order(
        round_end
            .racers
            .iter()
            .filter(|r| CountedRace::Last.time(r).is_some())
            .map(|r| r.account_id.as_str()),
        round_end,
        CountedRace::Last,
    )
}

fn round_points(host: &dyn Host, round_end: &RoundEnd) -> RoundPoints {
    let order: Vec<u32> = finishers(round_end)
        .iter()
        .filter_map(|account_id| host.team_of(account_id))
        .collect();
    let outcome = Outcome::of(&order);
    let (first, other) = outcome.points();
    let teams = host
        .teams()
        .iter()
        .map(|team| match order.first() {
            Some(&leader) if leader == team.team_id => (team.team_id, first),
            Some(_) => (team.team_id, other),
            None => (team.team_id, 0),
        })
        .collect();
    RoundPoints {
        outcome,
        leader: order.first().copied(),
        teams,
    }
}

/// A finisher gets the points of the team, everyone else none.
fn player_points(
    host: &dyn Host,
    points: &RoundPoints,
    round_end: &RoundEnd,
    account_id: &str,
) -> i32 {
    let finished = round_end
        .racer(account_id)
        .is_some_and(|racer| CountedRace::Last.time(racer).is_some());
    match (finished, host.team_of(account_id)) {
        (true, Some(team)) => points
            .teams
            .iter()
            .find(|(id, _)| *id == team)
            .map_or(0, |(_, points)| *points),
        _ => 0,
    }
}

/// Every team of the match is in the standings.
fn join_teams(host: &dyn Host, standings: &mut Standings) {
    for team in host.teams() {
        if !standings.teams.iter().any(|t| t.team_id == team.team_id) {
            standings.teams.push(TeamEntry {
                team_id: team.team_id,
                position: 0,
                score: 0,
                map_score: 0,
                label: None,
            });
        }
    }
}

/// By match points, then by the points on the map, finally by team, so nobody ties.
fn rank_teams(standings: &mut Standings) {
    standings
        .teams
        .sort_by_key(|team| (-team.score, -team.map_score, team.team_id));
    for (place, team) in standings.teams.iter_mut().enumerate() {
        team.position = place as u32 + 1;
    }
}

impl Tmwt2v2 {
    fn round_end(
        &self,
        host: &dyn Host,
        standings: &mut Standings,
        round_end: RoundEnd,
    ) -> Vec<Command> {
        // Rounds which do not count give no points.
        if round_end.invalid {
            return Vec::new();
        }
        join_teams(host, standings);
        for racer in &round_end.racers {
            standings.join(&racer.account_id);
        }

        let points = round_points(host, &round_end);
        for racer in &round_end.racers {
            let earned = player_points(host, &points, &round_end, &racer.account_id);
            standings.award(&racer.account_id, earned);
        }
        for &(team_id, earned) in &points.teams {
            if let Some(team) = standings.teams.iter_mut().find(|t| t.team_id == team_id) {
                team.map_score += earned;
            }
        }
        standings.round_teams = round_teams(&points);

        standings.rounds_on_map += 1;
        standings.rank(&round_end, CountedRace::Last);
        standings.place_round(&round_end, CountedRace::Last);

        let mut commands: Vec<Command> = points
            .outcome
            .announcement(&self.name_of(host, points.leader.unwrap_or(0)))
            .map(Command::ChatMessage)
            .into_iter()
            .collect();
        let map_won = self.map_winner(standings);
        if let Some(winner) = map_won
            && let Some(team) = standings.teams.iter_mut().find(|t| t.team_id == winner)
        {
            team.score += 1;
        }
        // Also with a limit lowered during the match, below the maps a team won before.
        let champion = standings
            .teams
            .iter()
            .filter(|team| team.score >= self.settings.match_points_limit)
            .max_by_key(|team| (team.score, team.map_score, std::cmp::Reverse(team.team_id)))
            .map(|team| team.team_id);
        if let Some(champion) = champion {
            for team in &mut standings.teams {
                team.label = (team.team_id == champion).then(Label::winner);
            }
            rank_teams(standings);
            standings.crown_leaders();
            standings.info = format!("{} won the match", self.name_of(host, champion));
            commands.push(Command::EndMatch);
            return commands;
        }
        if map_won.is_some() {
            commands.push(Command::EndMap);
        }
        self.label_teams(standings);
        rank_teams(standings);
        standings.info = self.info(standings);
        commands
    }

    /// The team which won the map: at the limit with more points than everyone else.
    fn map_winner(&self, standings: &Standings) -> Option<u32> {
        let mut by_points: Vec<&TeamEntry> = standings.teams.iter().collect();
        by_points.sort_by_key(|team| -team.map_score);
        match by_points[..] {
            [first, second, ..] if first.map_score > second.map_score => {
                (first.map_score >= self.settings.map_points_limit).then_some(first.team_id)
            }
            [only] => (only.map_score >= self.settings.map_points_limit).then_some(only.team_id),
            _ => None,
        }
    }

    /// Both teams level at the limit, the map goes on until one leads.
    fn overtime(&self, standings: &Standings) -> bool {
        standings.teams.len() > 1
            && standings
                .teams
                .iter()
                .all(|team| team.map_score >= self.settings.map_points_limit)
            && self.map_winner(standings).is_none()
    }

    fn label_teams(&self, standings: &mut Standings) {
        let overtime = self.overtime(standings);
        for team in &mut standings.teams {
            team.label = overtime.then(|| Label::new("Overtime", Tone::Neutral));
        }
    }

    fn info(&self, standings: &Standings) -> String {
        let map = standings.maps_played + 1;
        let limit = self.settings.map_points_limit;
        if self.overtime(standings) {
            format!("Map {map}, overtime")
        } else {
            format!("Map {map}, first to {limit}")
        }
    }

    fn name_of(&self, host: &dyn Host, team_id: u32) -> String {
        host.teams()
            .into_iter()
            .find(|team| team.team_id == team_id)
            .map_or_else(|| format!("Team {team_id}"), |team| team.name)
    }
}

/// The teams of the round placed by their points.
fn round_teams(points: &RoundPoints) -> Vec<TeamEntry> {
    let mut teams: Vec<TeamEntry> = points
        .teams
        .iter()
        .map(|&(team_id, score)| TeamEntry {
            team_id,
            position: 0,
            score,
            map_score: 0,
            label: None,
        })
        .collect();
    teams.sort_by_key(|team| (-team.score, team.team_id));
    for (place, team) in teams.iter_mut().enumerate() {
        team.position = place as u32 + 1;
    }
    teams
}

#[cfg(test)]
mod tests;
