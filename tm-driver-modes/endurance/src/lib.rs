//! Endurance mode for the wasm driver.
//!
//! One long race in which the player who collects the most checkpoints wins. The
//! race is a single round on a map with laps: with infinite laps it ends with its
//! time limit, with a number of laps once everyone finished or the finish timeout
//! after the first finish ran out. Players cannot give up, respawning at a
//! checkpoint stays possible.
//!
//! Players count the checkpoints of every race of the round, e.g. also of the race
//! before they left and came back. Equal ones go to whoever got there first: the
//! race times at their last checkpoints added up, then the account. The round
//! shows live with the lap every player is in, and the mode brings the live
//! standings, which show the round instead of the match. Once the race ended, its
//! order is the result of the match.
//!
//! On maps without laps the finish still ends a race. With infinite laps players
//! start again, so the time limit decides there too.

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};
use tm_driver_sdk::{
    Command, Entry, Event, Label, Laps, Mode, RaceRules, Racer, RoundEnd, RoundFormat, Standings,
    Status, Tone,
    settings::{FinishTimeoutSetting, WarmUpSetting},
};

tm_driver_sdk::export_mode!(Endurance);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Laps of the race. Without, the laps never end and the time limit ends the race.
    pub laps: Option<u32>,
    /// How long a race with infinite laps lasts in milliseconds.
    pub time_limit_ms: u32,
    /// How long the others have to finish once the first player completed the laps.
    pub finish_timeout: FinishTimeoutSetting,
    /// The warmup before the race.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            laps: None,
            time_limit_ms: 3_600_000,
            finish_timeout: FinishTimeoutSetting::Automatic,
            warmup: WarmUpSetting::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Endurance {
    settings: Settings,
}

impl Mode for Endurance {
    const NAME: &'static str = "Endurance";
    /// Every setting, for the next race: a race that did not count or the next match.
    const RELOADABLE: &'static [&'static str] =
        &["laps", "time_limit_ms", "finish_timeout", "warmup"];

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        match settings.laps {
            Some(0) => Err("Invalid settings: a race has at least one lap".into()),
            None if settings.time_limit_ms == 0 => {
                Err("Invalid settings: a race with infinite laps needs a time limit".into())
            }
            _ => Ok(Self { settings }),
        }
    }

    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            // The script waits here, so this is where the rules for the map are set.
            // New settings send them again for the next race.
            Event::MapStart | Event::SettingsChanged => {
                standings.info = self.info();
                vec![
                    Command::SetRoundFormat(self.round_format()),
                    Command::SetRaceRules(RaceRules {
                        laps: self.settings.laps.map_or(Laps::Infinite, Laps::Count),
                        give_up: false,
                    }),
                    Command::SetWarmUp(self.settings.warmup.into()),
                ]
            }
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            // Everyone who races shows up in the standings and the live round.
            Event::RaceStart(account_id) => {
                standings.join(&account_id);
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MapEnd | Event::MatchEnd => Vec::new(),
        }
    }

    fn live_round(&self, standings: &Standings, races: &RoundEnd) -> Vec<Entry> {
        self.place(&races.taking_part(standings))
    }
}

impl Endurance {
    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // A race that does not count, e.g. after a pause, is driven again.
        if round_end.invalid {
            return Vec::new();
        }
        for racer in &round_end.racers {
            standings.join(&racer.account_id);
        }
        let round = self.place(&round_end.taking_part(standings));
        // The race is the match.
        for entry in &round {
            let player = standings.join(&entry.account_id);
            player.position = entry.position;
            player.score = entry.score;
            player.time = entry.time;
        }
        standings
            .entries
            .sort_by_key(|player| (player.position == 0, player.position));
        standings.round = round;
        standings.rounds_on_map += 1;
        standings.crown_leaders();
        vec![Command::EndMatch]
    }

    /// The racers by the checkpoints they collected, equal ones by who got there
    /// first. Their points are the checkpoints, their time the race times at their
    /// last checkpoints added up and their label the lap they are in.
    fn place(&self, races: &RoundEnd) -> Vec<Entry> {
        let mut progress: Vec<Progress> = races.racers.iter().map(Progress::of).collect();
        progress.sort_by_key(|p| (Reverse(p.checkpoints), p.time, p.account_id.clone()));
        progress
            .into_iter()
            .enumerate()
            .map(|(index, progress)| Entry {
                position: index as u32 + 1,
                score: i32::try_from(progress.checkpoints).unwrap_or(i32::MAX),
                time: (progress.checkpoints > 0).then_some(progress.time),
                status: Status::Playing,
                label: Some(self.lap(&progress)),
                account_id: progress.account_id,
            })
            .collect()
    }

    /// The lap a player is in, e.g. `Lap 3 / 5`, or that they finished. With
    /// infinite laps on a map without laps every finish counts as a lap.
    fn lap(&self, progress: &Progress) -> Label {
        let lap = progress.laps.saturating_add(1);
        match self.settings.laps {
            Some(_) if progress.finished => Label::new("Finished", Tone::Positive),
            Some(laps) => Label::new(format!("Lap {} / {laps}", lap.min(laps)), Tone::Neutral),
            None => Label::new(format!("Lap {lap}"), Tone::Neutral),
        }
    }

    /// Players never finish with infinite laps, so time attack rounds without
    /// restarts end with the time limit. With laps the round ends once everyone
    /// finished or the finish timeout ran out.
    fn round_format(&self) -> RoundFormat {
        match self.settings.laps {
            Some(_) => RoundFormat::SingleAttempt(self.settings.finish_timeout.into()),
            None => RoundFormat::TimeAttack(self.settings.time_limit_ms),
        }
    }

    fn info(&self) -> String {
        match self.settings.laps {
            Some(1) => "1 lap".into(),
            Some(laps) => format!("{laps} laps"),
            None => duration(self.settings.time_limit_ms),
        }
    }
}

/// How far a player got in the round over all their races.
struct Progress {
    account_id: String,
    checkpoints: u32,
    /// The race times at the last checkpoints of the races added up.
    time: u32,
    laps: u32,
    /// A race reached the finish after its laps.
    finished: bool,
}

impl Progress {
    fn of(racer: &Racer) -> Self {
        let races = &racer.races;
        Self {
            account_id: racer.account_id.clone(),
            checkpoints: races.iter().map(|race| race.checkpoints.len() as u32).sum(),
            time: races
                .iter()
                .filter_map(|race| race.checkpoints.last())
                .fold(0, |sum, &time| sum.saturating_add(time)),
            laps: races.iter().map(|race| race.laps).sum(),
            finished: races.iter().any(|race| race.finished),
        }
    }
}

/// A duration like a race time without milliseconds, e.g. `1:30:00` or `45:00`.
fn duration(milliseconds: u32) -> String {
    let seconds = milliseconds / 1_000;
    match seconds / 3_600 {
        0 => format!("{}:{:02}", seconds / 60, seconds % 60),
        hours => format!("{hours}:{:02}:{:02}", seconds / 60 % 60, seconds % 60),
    }
}

#[cfg(test)]
mod tests;
