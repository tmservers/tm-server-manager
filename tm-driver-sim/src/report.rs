//! What happened in a simulation as text, for the snapshots and to read along.
//!
//! One block per round: the races as the driver saw them, the places of the round,
//! the standings after it and what the driver did. Callbacks the driver only
//! collects, e.g. waypoints, are summed up in the races.

use std::fmt::Write;

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
    observe::{Observer, SeenHold},
    world::{Races, World},
};

pub(crate) struct Report {
    text: String,
}

impl Report {
    /// The header of a report: the settings and, for a simulation, its world.
    pub(crate) fn new(header: String, settings: &str, world: Option<&World>) -> Self {
        let mut report = Self {
            text: String::new(),
        };
        report.line(header);
        report.line(format!("settings {settings}"));
        if let Some(world) = world {
            report.world(world);
        }
        report
    }

    fn world(&mut self, world: &World) {
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
        self.line(format!("maps {}", maps.join(" · ")));
        if !world.playlist.is_empty() {
            self.line(format!("playlist {}", world.playlist.join(" ")));
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
        self.line(format!("players {}", players.join(" · ")));
        let Races { ties, idle } = world.races;
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
            ("pauses", faults.pauses),
        ];
        let chances: Vec<String> = chances
            .iter()
            .filter(|(_, per_mille)| *per_mille > 0)
            .map(|(name, per_mille)| format!("{name} {per_mille}‰"))
            .collect();
        if !chances.is_empty() {
            self.line(format!("chances {}", chances.join(" · ")));
        }
    }

    fn line(&mut self, line: impl AsRef<str>) {
        self.text.push_str(line.as_ref());
        self.text.push('\n');
    }

    fn notes(&mut self, notes: &mut Vec<String>, indent: &str) {
        for note in notes.drain(..) {
            self.line(format!("{indent}! {note}"));
        }
    }

    /// Writes what an input changed. `notes` are what happened around it, e.g.
    /// players joining, and go into the next block.
    pub(crate) fn after(
        &mut self,
        input: &DriverInput,
        hold: Option<&SeenHold>,
        reply: &DriverReply,
        driver: &Driver,
        observer: &Observer,
        notes: &mut Vec<String>,
    ) {
        let DriverInput::Event(event) = input else {
            return;
        };
        match event {
            Event::StartMatchStart(start) => {
                self.notes(notes, "");
                self.line(format!("\nmatch {}", start.count));
            }
            Event::EndMatchStart(_) => self.line("match end"),
            Event::DriverHold(_) => {
                let Some(hold) = hold else {
                    return;
                };
                if !hold.first {
                    self.line(format!("  hold {} again", hold.hold.seq));
                    return;
                }
                match hold.hold.section {
                    DriverSection::MapStart => {
                        self.notes(notes, "");
                        let mut line = format!(
                            "map {} · {} · {}",
                            observer.map(),
                            format(observer.format()),
                            warm_up(observer.warm_up())
                        );
                        if let Some(next) = observer.next_map() {
                            write!(line, " · next {next}").unwrap();
                        }
                        self.line(line);
                    }
                    DriverSection::RoundEnd => self.round(hold, reply, driver, observer, notes),
                    DriverSection::MapEnd => {
                        self.notes(notes, "  ");
                        let mut line = "map end".to_string();
                        if let Some(next) = observer.next_map() {
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

    fn round(
        &mut self,
        hold: &SeenHold,
        reply: &DriverReply,
        driver: &Driver,
        observer: &Observer,
        notes: &mut Vec<String>,
    ) {
        let counted = match (hold.hold.forced, hold.counts) {
            (true, _) => " · forced",
            (false, false) => " · without its start",
            (false, true) => "",
        };
        self.line(format!("round {}{counted}", observer.rounds()));
        self.notes(notes, "  ");

        let races: Vec<String> = observer
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
        notes: &mut Vec<String>,
        stopped: Option<&str>,
        inputs: usize,
        failure: Option<&Failure>,
    ) -> String {
        self.notes(notes, "");
        self.line("");
        match (failure, stopped) {
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
