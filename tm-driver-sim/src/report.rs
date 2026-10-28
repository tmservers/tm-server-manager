//! What happened in a simulation as text, for the snapshots and to read along.
//!
//! One block per round: the races as the driver saw them, the places of the round,
//! the standings after it and what the driver did. Callbacks the driver only
//! collects, e.g. waypoints, are summed up in the races.

use std::{collections::BTreeSet, fmt::Write};

use tm_server_types::{
    event::{DriverSection, Event},
    method::{
        DriverCommand, DriverEntry, DriverFinishTimeout, DriverRoundFormat, DriverStatus,
        DriverWarmUp,
    },
};
use tm_wasm_driver::{Driver, DriverInput, DriverReply};

use crate::{
    Failure,
    server::Server,
    world::{Races, World},
};

pub(crate) struct Report {
    text: String,
    holds: BTreeSet<(i32, u32)>,
}

impl Report {
    pub(crate) fn new(world: &World, seed: Option<u64>) -> Self {
        let mut report = Self {
            text: String::new(),
            holds: BTreeSet::new(),
        };
        match seed {
            Some(seed) => report.line(format!("{} · seed {seed}", world.mode.name())),
            None => report.line(format!("{} · scenario", world.mode.name())),
        }
        report.line(format!("settings {}", world.settings));
        let maps: Vec<String> = world
            .maps
            .iter()
            .map(|map| {
                format!(
                    "{} {} cp {}",
                    map.uid,
                    map.sectors.len(),
                    time(map.author_time())
                )
            })
            .collect();
        report.line(format!("maps {}", maps.join(" · ")));
        if !world.playlist.is_empty() {
            report.line(format!("playlist {}", world.playlist.join(" ")));
        }
        let players: Vec<String> = world
            .players
            .iter()
            .map(|player| {
                let mut text = player.account_id.clone();
                if player.name != player.account_id {
                    write!(text, " {}", player.name).unwrap();
                }
                if let Some(joins) = player.joins {
                    let when = if joins.during { "during" } else { "before" };
                    write!(text, " joins {when} round {}", joins.round).unwrap();
                }
                if let Some(leaves) = player.leaves {
                    let when = if leaves.during { "during" } else { "before" };
                    write!(text, " leaves {when} round {}", leaves.round).unwrap();
                }
                if player.watches {
                    text.push_str(" watches");
                }
                text
            })
            .collect();
        report.line(format!("players {}", players.join(" · ")));
        if let Races::Random { ties, idle } = world.races {
            let faults = &world.faults;
            let chances = [
                ("ties", ties),
                ("idle", idle),
                ("repeated holds", faults.repeated_holds),
                ("forced rounds", faults.forced_rounds),
                ("missed starts", faults.missed_starts),
                ("bridge restarts", faults.bridge_restarts),
                ("script restarts", faults.script_restarts),
                ("playlist changes", faults.playlist_changes),
            ];
            let chances: Vec<String> = chances
                .iter()
                .filter(|(_, per_mille)| *per_mille > 0)
                .map(|(name, per_mille)| format!("{name} {per_mille}‰"))
                .collect();
            if !chances.is_empty() {
                report.line(format!("chances {}", chances.join(" · ")));
            }
        }
        report
    }

    fn line(&mut self, line: impl AsRef<str>) {
        self.text.push_str(line.as_ref());
        self.text.push('\n');
    }

    fn notes(&mut self, server: &mut Server, indent: &str) {
        for note in server.take_notes() {
            self.line(format!("{indent}! {note}"));
        }
    }

    pub(crate) fn after(
        &mut self,
        input: &DriverInput,
        reply: &DriverReply,
        driver: &Driver,
        server: &mut Server,
    ) {
        let DriverInput::Event(event) = input else {
            return;
        };
        match event {
            Event::StartMatchStart(start) => {
                self.notes(server, "");
                self.line(format!("\nmatch {}", start.count));
            }
            Event::EndMatchStart(_) => self.line("match end"),
            Event::DriverHold(hold) => {
                if !self.holds.insert((hold.session, hold.seq)) {
                    self.line(format!("  hold {} again", hold.seq));
                    return;
                }
                match hold.section {
                    DriverSection::MapStart => {
                        self.notes(server, "");
                        let mut line = format!(
                            "map {} · {} · {}",
                            server.map_uid(),
                            format(server.format()),
                            warm_up(server.warm_up())
                        );
                        if let Some(next) = server.next_map() {
                            write!(line, " · next {next}").unwrap();
                        }
                        self.line(line);
                    }
                    DriverSection::RoundEnd => self.round(hold.forced, reply, driver, server),
                    DriverSection::MapEnd => {
                        self.notes(server, "  ");
                        let mut line = "map end".to_string();
                        if let Some(next) = server.next_map() {
                            write!(line, " · next {next}").unwrap();
                        }
                        self.line(line);
                        self.commands(&reply.commands);
                    }
                }
            }
            _ => {}
        }
    }

    fn round(&mut self, forced: bool, reply: &DriverReply, driver: &Driver, server: &mut Server) {
        let forced = if forced { " · forced" } else { "" };
        self.line(format!("round {}{forced}", server.rounds()));
        self.notes(server, "  ");

        let races: Vec<String> = server
            .truth()
            .racers
            .iter()
            .map(|(account_id, races)| {
                let races: Vec<String> = races
                    .iter()
                    .map(|race| match race.time() {
                        Some(finish) => time(finish),
                        None => format!("{}cp", race.checkpoints.len()),
                    })
                    .collect();
                format!("{account_id} {}", races.join(" "))
            })
            .collect();
        self.line(format!("  races  {}", or_none(&races)));
        if reply.round_result.is_some() {
            let round: Vec<String> = driver
                .last_round()
                .entries
                .iter()
                .map(|entry| format!("{} {} {}", entry.position, entry.account_id, entry.score))
                .collect();
            self.line(format!("  round  {}", or_none(&round)));
        }
        let standings = driver.standings();
        let entries: Vec<String> = standings.entries.iter().map(entry).collect();
        self.line(format!("  match  {}", or_none(&entries)));
        self.line(format!("  info   {}", standings.info));
        self.commands(&reply.commands);
    }

    /// What the driver made happen, without what it publishes.
    fn commands(&mut self, commands: &[DriverCommand]) {
        let mut blocked = Vec::new();
        let mut allowed = Vec::new();
        for command in commands {
            match command {
                DriverCommand::ChatMessage(message) => self.line(format!("  chat   {message}")),
                DriverCommand::SetCanSpawn(can_spawn) if can_spawn.can_spawn => {
                    allowed.push(can_spawn.account_id.as_str());
                }
                DriverCommand::SetCanSpawn(can_spawn) => {
                    blocked.push(can_spawn.account_id.as_str())
                }
                DriverCommand::EndMap => self.line("  end map"),
                DriverCommand::EndMatch => self.line("  end match"),
                _ => {}
            }
        }
        if !blocked.is_empty() {
            self.line(format!("  block  {}", blocked.join(" ")));
        }
        if !allowed.is_empty() {
            self.line(format!("  allow  {}", allowed.join(" ")));
        }
    }

    pub(crate) fn finish(
        mut self,
        driver: &Driver,
        server: &mut Server,
        inputs: usize,
        failure: Option<&Failure>,
    ) -> String {
        self.notes(server, "");
        self.line("");
        match (failure, server.stopped()) {
            (Some(failure), _) => self.line(format!(
                "FAILED after input {}: {}",
                failure.input, failure.message
            )),
            (None, Some(reason)) => self.line(format!("end · {reason}")),
            (None, None) => self.line("end"),
        }
        let standings = driver.standings();
        for entry in &standings.entries {
            self.line(format!("  {}", self::entry(entry)));
        }
        self.line(format!("inputs {inputs}"));
        self.text
    }
}

/// A player in the standings: position, account, points, the status if not playing
/// and the label.
fn entry(entry: &DriverEntry) -> String {
    let mut text = match entry.status {
        DriverStatus::Spectator => format!("- {} spectator", entry.account_id),
        _ => {
            let position = match entry.position {
                0 => "-".to_string(),
                position => position.to_string(),
            };
            format!("{position} {} {}", entry.account_id, entry.score)
        }
    };
    if entry.status == DriverStatus::Eliminated {
        text.push_str(" out");
    }
    if !entry.label.is_empty() {
        write!(text, " ({})", escape(&entry.label)).unwrap();
    }
    text
}

fn or_none(items: &[String]) -> String {
    if items.is_empty() {
        "-".into()
    } else {
        items.join(" · ")
    }
}

/// Icons of the game font, e.g. the hearts of a knockout, as escapes so the
/// snapshots stay readable.
fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{e000}'..='\u{f8ff}' => c.escape_unicode().to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// A race time like the game shows it, e.g. `1:02.345`.
pub(crate) fn time(milliseconds: u32) -> String {
    format!(
        "{}:{:02}.{:03}",
        milliseconds / 60_000,
        milliseconds / 1_000 % 60,
        milliseconds % 1_000
    )
}

fn format(format: DriverRoundFormat) -> String {
    match format {
        DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Automatic) => {
            "one try, automatic finish timeout".into()
        }
        DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Disabled) => {
            "one try, no finish timeout".into()
        }
        DriverRoundFormat::SingleAttempt(DriverFinishTimeout::Milliseconds(ms)) => {
            format!("one try, finish timeout {}", time(ms))
        }
        DriverRoundFormat::TimeAttack(limit) => format!("time attack {}", time(limit)),
    }
}

fn warm_up(warm_up: DriverWarmUp) -> String {
    match (warm_up.rounds, warm_up.duration_ms) {
        (0, _) => "no warmup".into(),
        (rounds, 0) => format!("warmup {rounds} × one try"),
        (rounds, duration) => format!("warmup {rounds} × {}", time(duration)),
    }
}
