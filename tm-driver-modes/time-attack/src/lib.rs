//! Time attack mode for the wasm driver.
//!
//! Every map is one time attack round: players restart as often as they like
//! until the time limit runs out and their best time counts. The ranking of
//! every map is turned into points so the standings can add up several maps.
//! By default the best of N finishers gets N points, the second N - 1 and so
//! on, which keeps the order of the times.
//!
//! Nobody ties: equal best times go to the better times at the checkpoints of
//! those races and finally to the name, players with the same points by their
//! race on the last map.

use serde::Deserialize;
use tm_driver_sdk::{
    Command, CountedRace, Event, Mode, RoundEnd, RoundFormat, Standings,
    settings::{WarmUpSetting, maps_per_match},
};

tm_driver_sdk::export_mode!(TimeAttack);

#[derive(Debug, Clone, Deserialize)]
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
            // of the previous mode might still be active.
            Event::MapStart => {
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
                standings.list_newcomers();
                Vec::new()
            }
            Event::MatchStart | Event::RoundStart | Event::MatchEnd => Vec::new(),
        }
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
        for result in round_end.results.iter().filter(|r| r.started) {
            standings.join(&result.account_id);
        }

        let finishers = tm_driver_sdk::race_order(
            round_end
                .results
                .iter()
                .filter(|r| r.best_time.is_some())
                .map(|r| r.account_id.as_str()),
            &round_end.results,
            CountedRace::Best,
        );
        for (rank, account_id) in finishers.iter().enumerate() {
            standings.award(account_id, self.points_for(rank, finishers.len()));
        }

        standings.rounds_on_map += 1;
        standings.rank(&round_end.results, CountedRace::Best);
        standings.place_round(&round_end.results, CountedRace::Best);
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
