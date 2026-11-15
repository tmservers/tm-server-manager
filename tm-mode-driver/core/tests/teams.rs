//! Matches of teams: the host knows the teams, publishes them and the team standings
//! a mode keeps. A small mode of teams stands in for a real one.

mod common;

use common::{hold, map_start, match_start, play, player_info, race, round_start};
use serde::{Deserialize, Serialize};
use tm_mode_driver::{Driver, DriverInput, DriverLimits};
use tm_mode_driver_sdk::{Command, Event, Host, Mode, Native, Standings, TeamEntry};
use tm_server_types::{
    event::{DriverSection, DriverTeam, DriverTeamResult},
    method::{DriverCommand, DriverLabelTone, DriverNetStandings, DriverNetTeams, DriverTeamEntry},
};

/// Every finisher earns its team a point, the team with more points leads.
struct TeamPoints;

#[derive(Default, Serialize, Deserialize)]
struct NoSettings {}

impl Mode for TeamPoints {
    const NAME: &'static str = "Team Points";
    type Settings = NoSettings;

    fn init(_: NoSettings) -> Result<Self, String> {
        Ok(TeamPoints)
    }

    fn step(&self, host: &dyn Host, standings: &mut Standings, event: Event) -> Vec<Command> {
        if let Event::RoundEnd(round_end) = event {
            let mut round: Vec<TeamEntry> = host
                .teams()
                .iter()
                .map(|team| TeamEntry {
                    team_id: team.team_id,
                    position: 0,
                    score: 0,
                    map_score: 0,
                    label: None,
                })
                .collect();
            for racer in &round_end.racers {
                let finished = racer.races.iter().any(|race| race.finished);
                if let (true, Some(team)) = (finished, host.team_of(&racer.account_id))
                    && let Some(entry) = round.iter_mut().find(|t| t.team_id == team)
                {
                    entry.score += 1;
                }
            }
            for entry in &round {
                match standings
                    .teams
                    .iter_mut()
                    .find(|t| t.team_id == entry.team_id)
                {
                    Some(team) => {
                        team.score += entry.score;
                        team.map_score += entry.score;
                    }
                    None => standings.teams.push(TeamEntry {
                        map_score: entry.score,
                        ..entry.clone()
                    }),
                }
            }
            rank(&mut round);
            rank(&mut standings.teams);
            standings.round_teams = round;
        }
        Vec::new()
    }
}

/// More points first, equal ones by team id. The teams stay in their order, the host
/// orders them by position.
fn rank(teams: &mut [TeamEntry]) {
    let mut order: Vec<(i32, u32)> = teams.iter().map(|t| (-t.score, t.team_id)).collect();
    order.sort();
    for team in teams.iter_mut() {
        let place = order
            .iter()
            .position(|&(_, id)| id == team.team_id)
            .unwrap();
        team.position = place as u32 + 1;
    }
}

fn team(team_id: u32, name: &str, players: &[&str]) -> DriverTeam {
    DriverTeam {
        team_id,
        name: name.into(),
        short_name: name[..1].into(),
        logo_url: format!("https://logos/{team_id}"),
        flag_url: String::new(),
        players: players.iter().map(|p| p.to_string()).collect(),
    }
}

fn teams() -> Vec<DriverTeam> {
    vec![team(70, "Red", &["a", "b"]), team(40, "Blue", &["c", "d"])]
}

fn driver() -> Driver {
    Driver::start(&Native::<TeamPoints>::new(), "{}", DriverLimits::default()).unwrap()
}

fn published<T: serde::de::DeserializeOwned>(
    commands: &[DriverCommand],
    pick: fn(&DriverCommand) -> Option<&String>,
) -> Option<T> {
    commands
        .iter()
        .filter_map(pick)
        .last()
        .map(|json| serde_json::from_str(json).unwrap())
}

fn teams_json(command: &DriverCommand) -> Option<&String> {
    match command {
        DriverCommand::SetTeams(json) => Some(json),
        _ => None,
    }
}

fn standings_json(command: &DriverCommand) -> Option<&String> {
    match command {
        DriverCommand::SetStandings(json) => Some(json),
        _ => None,
    }
}

#[test]
fn the_teams_go_out_by_id_with_their_players_by_index() {
    let mut driver = driver();
    // b is known before the teams, c and d only from them.
    driver.handle(&player_info("b", "Bea")).unwrap();

    let reply = driver.apply(&DriverInput::Teams(teams())).unwrap();

    let sent: DriverNetTeams = published(&reply.commands, teams_json).expect("the teams went out");
    assert_eq!(sent.ids, [70, 40]);
    assert_eq!(sent.names, ["Red", "Blue"]);
    assert_eq!(sent.short_names, ["R", "B"]);
    assert_eq!(sent.logo_urls, ["https://logos/70", "https://logos/40"]);
    let index = |account: &str| {
        driver
            .users()
            .users
            .iter()
            .find(|u| u.account_id == account)
            .unwrap()
            .index
    };
    let members: Vec<(u32, u32)> = sent
        .players
        .iter()
        .copied()
        .zip(sent.player_teams)
        .collect();
    assert_eq!(
        members,
        [
            (index("a"), 70),
            (index("b"), 70),
            (index("c"), 40),
            (index("d"), 40)
        ]
    );
    assert_eq!(index("b"), 0);

    // The same teams again change nothing.
    let again = driver.apply(&DriverInput::Teams(teams())).unwrap();
    assert!(published::<DriverNetTeams>(&again.commands, teams_json).is_none());
}

#[test]
fn a_match_of_players_sends_no_teams() {
    let mut driver = driver();
    let mut commands = driver.apply(&DriverInput::Resumed).unwrap().commands;
    for event in [match_start(), player_info("a", "Ant")] {
        commands.extend(driver.handle(&event).unwrap().commands);
    }

    assert!(published::<DriverNetTeams>(&commands, teams_json).is_none());
    assert!(!commands.is_empty(), "the rest went out");
    assert!(driver.standings().teams.is_empty());
}

#[test]
fn modes_rank_teams_by_the_teams_the_host_knows() {
    let mut driver = driver();
    driver.apply(&DriverInput::Teams(teams())).unwrap();
    play(
        &mut driver,
        &[
            match_start(),
            map_start("m1"),
            hold(1, DriverSection::MapStart),
        ],
    );
    driver.handle(&round_start(1)).unwrap();
    play(&mut driver, &race("a", 30_000));
    play(&mut driver, &race("c", 31_000));
    play(&mut driver, &race("d", 32_000));

    let reply = driver.handle(&hold(2, DriverSection::RoundEnd)).unwrap();

    let entry = |team_id, position, score| DriverTeamEntry {
        team_id,
        position,
        score,
        map_score: score,
        label: String::new(),
        label_tone: DriverLabelTone::Neutral,
    };
    assert_eq!(driver.standings().teams, [entry(40, 1, 2), entry(70, 2, 1)]);
    // The round only has points of the round.
    let round = |team_id, position, score| DriverTeamEntry {
        map_score: 0,
        ..entry(team_id, position, score)
    };
    assert_eq!(driver.round().teams, [round(40, 1, 2), round(70, 2, 1)]);
    assert_eq!(
        reply.round_result.unwrap().teams,
        [
            DriverTeamResult {
                team_id: 40,
                points: 2,
                position: 1
            },
            DriverTeamResult {
                team_id: 70,
                points: 1,
                position: 2
            },
        ]
    );
    let sent: DriverNetStandings =
        published(&reply.commands, standings_json).expect("the standings went out");
    assert_eq!(sent.teams.teams, [40, 70]);
    assert_eq!(sent.teams.scores, [2, 1]);
    assert_eq!(sent.teams.positions, [1, 2]);

    // A new match starts without the scores of the teams.
    play(&mut driver, &[match_start()]);
    assert!(driver.standings().teams.is_empty());
}
