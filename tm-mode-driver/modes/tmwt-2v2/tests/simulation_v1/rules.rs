//! The order in which the teams finish gives the points of a round, the points of a
//! map decide the map and the maps the match.

use std::collections::BTreeMap;

use tm_mode_driver_sim::rules::{
    Counted, DriverStandings, DriverTeam, LiveRound, MapEnd, RoundEnd, Rules, ends_map, ends_match,
    in_race_order, live_in_race_order, racers_placed,
};
use tm_mode_tmwt_2v2::v1::Settings;

pub struct Tmwt2v2 {
    settings: Settings,
    /// Maps won by team.
    maps: BTreeMap<u32, i32>,
}

impl Tmwt2v2 {
    pub fn boxed(settings: &str) -> Result<Box<dyn tm_mode_driver_sim::rules::Rules>, String> {
        Ok(Box::new(Self {
            settings: tm_mode_driver_sim::rules::settings(settings)?,
            maps: BTreeMap::new(),
        }))
    }
}

/// The team of a player.
fn team_of(teams: &[DriverTeam], account_id: &str) -> Option<u32> {
    teams
        .iter()
        .find(|team| team.players.iter().any(|p| p == account_id))
        .map(|team| team.team_id)
}

/// The points of every team for the teams of the finishers in the order they finished,
/// as the rules of TMWT list them.
fn expected(teams: &[DriverTeam], finishers: &[u32]) -> BTreeMap<u32, i32> {
    let mut points: BTreeMap<u32, i32> = teams.iter().map(|t| (t.team_id, 0)).collect();
    let Some(&first) = finishers.first() else {
        return points;
    };
    let same: Vec<bool> = finishers.iter().map(|&team| team == first).collect();
    let (first_points, other_points) = match same[..] {
        // A A …, or only A finished.
        [true] | [true, true, ..] => (3, 0),
        // A B, A B A …
        [true, false] | [true, false, true, ..] => (2, 1),
        // A B B …
        _ => (1, 1),
    };
    for (team, earned) in &mut points {
        *earned = if *team == first {
            first_points
        } else {
            other_points
        };
    }
    points
}

/// The teams of the finishers of the round in its order.
fn finisher_teams(
    teams: &[DriverTeam],
    round: &tm_mode_driver_sim::rules::DriverRound,
) -> Vec<u32> {
    round
        .entries
        .iter()
        .filter(|entry| entry.time >= 0)
        .filter_map(|entry| team_of(teams, &entry.account_id))
        .collect()
}

fn map_points(standings: &DriverStandings) -> BTreeMap<u32, i32> {
    standings
        .teams
        .iter()
        .map(|t| (t.team_id, t.map_score))
        .collect()
}

impl Rules for Tmwt2v2 {
    fn match_start(&mut self) {
        self.maps.clear();
    }

    fn settings_changed(&mut self, settings: &str) -> Result<(), String> {
        self.settings = tm_mode_driver_sim::rules::settings(settings)?;
        Ok(())
    }

    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        in_race_order(end, Counted::Last)?;
        racers_placed(end)?;
        if end.after.teams.len() != end.teams.len() {
            return Err(format!(
                "{} teams play but the standings have {}",
                end.teams.len(),
                end.after.teams.len()
            ));
        }

        let points = expected(end.teams, &finisher_teams(end.teams, end.round));
        let round: BTreeMap<u32, i32> = end
            .round
            .teams
            .iter()
            .map(|t| (t.team_id, t.score))
            .collect();
        if round != points {
            return Err(format!(
                "The teams finished {:?}, which gives {points:?}, not {round:?}",
                finisher_teams(end.teams, end.round)
            ));
        }
        // Finishers get the points of their team, everyone else none.
        for entry in &end.round.entries {
            let team = team_of(end.teams, &entry.account_id);
            let earned = match team {
                Some(team) if entry.time >= 0 => points[&team],
                _ => 0,
            };
            if entry.score != earned {
                return Err(format!(
                    "{} got {} points in the round instead of the {earned} of its team",
                    entry.account_id, entry.score
                ));
            }
        }

        let before = map_points(end.before);
        let after = map_points(end.after);
        let mut on_map: BTreeMap<u32, i32> = BTreeMap::new();
        for (team, earned) in &points {
            on_map.insert(*team, before.get(team).copied().unwrap_or(0) + earned);
        }
        if after != on_map {
            return Err(format!(
                "The teams have {after:?} points on the map instead of {on_map:?}"
            ));
        }

        let limit = self.settings.map_points_limit;
        let mut ranked: Vec<i32> = on_map.values().copied().collect();
        ranked.sort_by_key(|points| -points);
        let won = match ranked[..] {
            [first, second, ..] => first >= limit && first > second,
            [only] => only >= limit,
            [] => false,
        };
        let winner = won.then(|| *on_map.iter().max_by_key(|(_, p)| **p).unwrap().0);
        if let Some(winner) = winner {
            *self.maps.entry(winner).or_default() += 1;
        }
        for team in &end.after.teams {
            let maps = self.maps.get(&team.team_id).copied().unwrap_or(0);
            if team.score != maps {
                return Err(format!(
                    "Team {} has {} match points instead of the {maps} maps it won",
                    team.team_id, team.score
                ));
            }
        }
        // By match points, then by points on the map, finally by team.
        let mut order: Vec<(i32, i32, u32)> = end
            .after
            .teams
            .iter()
            .map(|t| (-t.score, -t.map_score, t.team_id))
            .collect();
        order.sort();
        let ranked: Vec<u32> = end.after.teams.iter().map(|t| t.team_id).collect();
        if ranked != order.iter().map(|o| o.2).collect::<Vec<_>>() {
            return Err(format!("The teams are not ranked: {ranked:?}"));
        }

        let match_won = self
            .maps
            .values()
            .any(|maps| *maps >= self.settings.match_points_limit);
        if ends_match(end.commands) != match_won {
            return Err(format!(
                "Maps won {:?} with {} to win, but the match ends: {}",
                self.maps,
                self.settings.match_points_limit,
                ends_match(end.commands)
            ));
        }
        if ends_map(end.commands) != (won && !match_won) {
            return Err(format!(
                "The map is won: {won}, but it ends: {}",
                ends_map(end.commands)
            ));
        }
        Ok(())
    }

    /// The finishers so far have the points of their team as if the round ended now.
    fn live_round(&mut self, live: &LiveRound) -> Result<(), String> {
        live_in_race_order(live, Counted::Last)?;
        let points = expected(live.teams, &finisher_teams(live.teams, live.round));
        for entry in &live.round.entries {
            let earned = match team_of(live.teams, &entry.account_id) {
                Some(team) if entry.time >= 0 => points[&team],
                _ => 0,
            };
            if entry.score != earned {
                return Err(format!(
                    "{} shows {} points live instead of the {earned} of its team",
                    entry.account_id, entry.score
                ));
            }
        }
        Ok(())
    }

    /// A map only ends once a team won it, and the match never ends with it.
    fn map_end(&mut self, end: &MapEnd) -> Result<(), String> {
        if ends_match(end.commands) {
            return Err("The match ends with the round, not with the map".into());
        }
        Ok(())
    }
}
