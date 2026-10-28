//! Rounds mode for the wasm driver.
//!
//! Follows the rules of the Nadeo Rounds mode: finishers get points by their
//! position, the map ends after a number of rounds and the match ends after a
//! number of maps or once a player reached the points limit.
//!
//! The points live in the standings the host keeps and hands over with every
//! event. The mode only applies its rules to them. Nobody ties: equal races go to
//! the better times at the checkpoints and finally to the name, players with the
//! same points by their race in the last round.

use serde::Deserialize;
use tm_driver_sdk::{
    Command, CountedRace, Event, Mode, RoundEnd, RoundFormat, Standings,
    settings::{FinishTimeoutSetting, WarmUpSetting, maps_per_match},
};

tm_driver_sdk::export_mode!(Rounds);

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Points by finish position. Finishers beyond the list get the last value.
    pub points_repartition: Vec<i32>,
    /// Valid rounds played on every map. 0 means no limit.
    pub rounds_per_map: u32,
    /// Maps played in the match. By default the playlist once, one map without a
    /// playlist. 0 means no limit.
    pub maps_per_match: Option<u32>,
    /// The match ends once a player reached this many points. 0 disables it.
    pub points_limit: i32,
    pub finish_timeout: FinishTimeoutSetting,
    /// The warmup before the first round of every map.
    pub warmup: WarmUpSetting,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            points_repartition: vec![10, 6, 4, 3, 2, 1],
            rounds_per_map: 5,
            maps_per_match: None,
            points_limit: 0,
            finish_timeout: FinishTimeoutSetting::Automatic,
            warmup: WarmUpSetting::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Rounds {
    settings: Settings,
}

impl Mode for Rounds {
    const NAME: &'static str = "Rounds";

    type Settings = Settings;

    fn init(settings: Settings) -> Result<Self, String> {
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
                    Command::SetRoundFormat(RoundFormat::SingleAttempt(
                        self.settings.finish_timeout.into(),
                    )),
                    Command::SetWarmUp(self.settings.warmup.into()),
                ]
            }
            Event::RoundEnd(round_end) => self.round_end(standings, round_end),
            Event::MapEnd => {
                if self.match_is_over(standings) {
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

impl Rounds {
    fn round_end(&self, standings: &mut Standings, round_end: RoundEnd) -> Vec<Command> {
        // Invalid rounds do not count.
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
                .filter(|r| r.previous_time.is_some())
                .map(|r| r.account_id.as_str()),
            &round_end.results,
            CountedRace::Last,
        );
        for (rank, account_id) in finishers.iter().enumerate() {
            standings.award(account_id, self.points_for(rank));
        }

        standings.rounds_on_map += 1;
        standings.rank(&round_end.results, CountedRace::Last);
        standings.place_round(&round_end.results, CountedRace::Last);
        standings.info = self.info(standings);

        if self.points_limit_reached(standings) {
            standings.crown_leaders();
            vec![Command::EndMatch]
        } else if self.settings.rounds_per_map > 0
            && standings.rounds_on_map >= self.settings.rounds_per_map
        {
            vec![Command::EndMap]
        } else {
            Vec::new()
        }
    }

    fn info(&self, standings: &Standings) -> String {
        let rounds = match self.settings.rounds_per_map {
            0 => format!("{} rounds", standings.rounds_on_map),
            limit => format!("{} / {limit} rounds", standings.rounds_on_map),
        };
        let map = standings.maps_played + 1;
        match maps_per_match(self.settings.maps_per_match) {
            1 => rounds,
            0 => format!("Map {map}, {rounds}"),
            maps => format!("Map {map} / {maps}, {rounds}"),
        }
    }

    fn points_for(&self, rank: usize) -> i32 {
        let repartition = &self.settings.points_repartition;
        repartition
            .get(rank)
            .or(repartition.last())
            .copied()
            .unwrap_or(0)
    }

    /// Players with the same points at the limit are told apart like every other tie.
    fn points_limit_reached(&self, standings: &Standings) -> bool {
        self.settings.points_limit > 0
            && standings
                .entries
                .iter()
                .any(|p| p.score >= self.settings.points_limit)
    }

    fn match_is_over(&self, standings: &Standings) -> bool {
        if self.points_limit_reached(standings) {
            true
        } else if let maps @ 1.. = maps_per_match(self.settings.maps_per_match) {
            standings.maps_played >= maps
        } else {
            // Without any limit the match ends after the first map.
            self.settings.rounds_per_map == 0 && self.settings.points_limit <= 0
        }
    }
}

#[cfg(test)]
mod tests;
