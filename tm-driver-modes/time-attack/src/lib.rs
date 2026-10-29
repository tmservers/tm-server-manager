//! Time attack mode for the wasm driver.
//!
//! Every map is one time attack round: players restart as often as they like
//! until the time limit runs out and their best time counts. The ranking of
//! every map is turned into points so the standings can add up several maps.
//! By default the best of N finishers gets N points, the second N - 1 and so
//! on, which keeps the order of the times.
//!
//! Nobody ties: equal best times go to the better times at the checkpoints of
//! those races and finally by account, players with the same points by their
//! race on the last map.

use serde::{Deserialize, Serialize};
use tm_driver_sdk::{
    Command, CountedRace, Entry, Event, Mode, RoundEnd, RoundFormat, Standings,
    settings::{WarmUpSetting, maps_per_match},
};

tm_driver_sdk::export_mode!(TimeAttack);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// How long every map is played in milliseconds.
    pub time_limit_ms: u32,
    /// Maps played in the match. By default the playlist once, one map without a
    /// playlist. 0 means no limit.
    pub maps_per_match: Option<u32>,
    /// Points by rank at the end of every map. Ranks beyond the list get the last value.
    /// When empty the best of N finishers gets N points, the second N - 1 and so on.
    pub points_repartition: Vec<i32>,
    /// The warmup before the time attack round of every map.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            time_limit_ms: 300_000,
            maps_per_match: None,
            points_repartition: Vec::new(),
            warmup: WarmUpSetting::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeAttack {
    settings: Settings,
}

impl Mode for TimeAttack {
    const NAME: &'static str = "Time Attack";
    /// The time limit and the warmup apply from the next map on.
    const RELOADABLE: &'static [&'static str] = &[
        "time_limit_ms",
        "maps_per_match",
        "points_repartition",
        "warmup",
    ];

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
        if settings.time_limit_ms == 0 {
            return Err("The time limit has to be at least one millisecond.".into());
        }
        Ok(Self { settings })
    }

    fn step(&self, standings: &mut Standings, event: Event) -> Vec<Command> {
        match event {
            // The script waits here, so this is where the rules for the map are set.
            // A config change keeps the match of the modescript going, so the rules
            // of the previous mode might still be active. New settings send them
            // again, the round format applies from the next round on.
            Event::MapStart | Event::SettingsChanged => {
                standings.info = self.info(standings);
                vec![
                    Command::SetRoundFormat(RoundFormat::TimeAttack(self.settings.time_limit_ms)),
                    Command::SetWarmUp(self.settings.warmup.into()),
                ]
            }
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            Event::MapEnd => {
                let maps = maps_per_match(self.settings.maps_per_match);
                if maps > 0 && standings.maps_played >= maps {
                    standings.crown_leaders();
                    vec![Command::EndMatch]
                } else {
                    Vec::new()
                }
            }
            // Everyone who races shows up in the standings before the first round ends.
            Event::RaceStart(account_id) => {
                standings.join(&account_id);
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
    }

    /// The best times so far with the points they would get.
    fn live_round(&self, standings: &Standings, races: &RoundEnd) -> Vec<Entry> {
        // Everyone else who raced joins at the round end.
        let races = races.taking_part(standings);
        let points = self.points(&races);
        standings.placed(&races, CountedRace::Best, |account_id| {
            points
                .iter()
                .find(|(finisher, _)| finisher == account_id)
                .map_or(0, |(_, points)| *points)
        })
    }
}

impl TimeAttack {
    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // An invalid round was cut short. The map continues with a new time attack
        // round.
        if round_end.invalid {
            return Vec::new();
        }

        // Everyone who raced shows up in the standings, with points or without.
        for racer in &round_end.racers {
            standings.join(&racer.account_id);
        }

        for (account_id, points) in self.points(&round_end) {
            standings.award(&account_id, points);
        }

        standings.rounds_on_map += 1;
        standings.rank(&round_end, CountedRace::Best);
        standings.place_round(&round_end, CountedRace::Best);
        standings.info = self.info(standings);
        // Every map is a single time attack round.
        vec![Command::EndMap]
    }

    fn info(&self, standings: &Standings) -> String {
        let map = standings.maps_played + 1;
        match maps_per_match(self.settings.maps_per_match) {
            0 => format!("Map {map}"),
            maps => format!("Map {map} / {maps}"),
        }
    }

    /// The finishers of a round by their best time with their points.
    fn points(&self, round_end: &RoundEnd) -> Vec<(String, i32)> {
        let finishers = tm_driver_sdk::race_order(
            round_end
                .racers
                .iter()
                .filter(|r| CountedRace::Best.time(r).is_some())
                .map(|r| r.account_id.as_str()),
            round_end,
            CountedRace::Best,
        );
        let count = finishers.len();
        finishers
            .into_iter()
            .enumerate()
            .map(|(rank, account_id)| (account_id, self.points_for(rank, count)))
            .collect()
    }

    fn points_for(&self, rank: usize, finishers: usize) -> i32 {
        let repartition = &self.settings.points_repartition;
        if repartition.is_empty() {
            return i32::try_from(finishers - rank).unwrap_or(i32::MAX);
        }
        repartition
            .get(rank)
            .or(repartition.last())
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
