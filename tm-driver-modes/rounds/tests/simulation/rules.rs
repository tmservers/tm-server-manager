//! Finishers get points by their place in the race, the points add up and a
//! points limit or the maps end the match.

use std::collections::BTreeMap;

use tm_driver_mode_rounds::Settings;
use tm_driver_sim::rules::{
    Counted, DriverStandings, MapEnd, RoundEnd, Rules, ends_map, ends_match, in_race_order,
    playing, points, racers_placed, ranked_by_points,
};

pub struct Rounds {
    settings: Settings,
    /// The points of every round of the match added up.
    totals: BTreeMap<String, i32>,
}

impl Rounds {
    /// The rules for a match with these settings.
    pub fn boxed(settings: &str) -> Result<Box<dyn Rules>, String> {
        let settings: Settings = tm_driver_sim::rules::settings(settings)?;
        Ok(Box::new(Self {
            settings,
            totals: BTreeMap::new(),
        }))
    }

    fn limit_reached(&self, end_scores: impl IntoIterator<Item = i32>) -> bool {
        let limit = self.settings.points_limit;
        limit > 0 && end_scores.into_iter().any(|score| score >= limit)
    }
}

impl Rules for Rounds {
    fn match_start(&mut self) {
        self.totals.clear();
    }

    fn settings_changed(&mut self, settings: &str) -> Result<(), String> {
        self.settings = tm_driver_sim::rules::settings(settings)?;
        Ok(())
    }

    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        in_race_order(end, Counted::Last)?;
        racers_placed(end)?;
        check_places(end, &self.settings.points_repartition)?;
        for entry in &end.round.entries {
            *self.totals.entry(entry.account_id.clone()).or_default() += entry.score;
        }
        for entry in playing(end.after) {
            let total = self.totals.get(&entry.account_id).copied().unwrap_or(0);
            if entry.score != total {
                return Err(format!(
                    "{} has {} points instead of the {total} of its rounds",
                    entry.account_id, entry.score
                ));
            }
        }
        ranked_by_points(end.after)?;

        let reached = self.limit_reached(end.after.entries.iter().map(|entry| entry.score));
        if reached != ends_match(end.commands) {
            return Err(format!(
                "The points limit {} was reached: {reached}, the match ends: {}",
                self.settings.points_limit,
                ends_match(end.commands)
            ));
        }
        let rounds_per_map = self.settings.rounds_per_map;
        let map_done = rounds_per_map > 0 && end.after.rounds_on_map >= rounds_per_map;
        if ends_map(end.commands) != (map_done && !reached) {
            return Err(format!(
                "{} of {rounds_per_map} rounds on the map, but the map ends: {}",
                end.after.rounds_on_map,
                ends_map(end.commands)
            ));
        }
        Ok(())
    }

    /// Every map ends after its rounds, and the maps of the match end it. The
    /// current map counts even when a new playlist has fewer maps.
    fn round_limit(&self, standings: &DriverStandings) -> Option<u32> {
        let maps = self
            .settings
            .maps_per_match
            .unwrap_or(standings.map_count.max(1));
        match (self.settings.rounds_per_map, maps) {
            (0, _) | (_, 0) => None,
            (rounds, maps) => Some(maps.max(standings.maps_played + 1) * rounds),
        }
    }

    fn map_end(&mut self, end: &MapEnd) -> Result<(), String> {
        let maps = self.settings.maps_per_match.unwrap_or(end.playlist.max(1));
        let over = self.limit_reached(end.after.entries.iter().map(|entry| entry.score))
            || match maps {
                0 => self.settings.rounds_per_map == 0 && self.settings.points_limit <= 0,
                maps => end.after.maps_played >= maps,
            };
        if over != ends_match(end.commands) {
            return Err(format!(
                "{} of {maps} maps played, but the match ends: {}",
                end.after.maps_played,
                ends_match(end.commands)
            ));
        }
        Ok(())
    }
}

/// The finishers get the points of their place and the time of their last race,
/// everyone else nothing.
fn check_places(end: &RoundEnd, repartition: &[i32]) -> Result<(), String> {
    for (place, entry) in end.round.entries.iter().enumerate() {
        let finish = end.truth.last_finish(&entry.account_id);
        let expected = if finish.is_some() {
            points(repartition, place)
        } else {
            0
        };
        if entry.score != expected {
            return Err(format!(
                "{} got {} points in place {}, not {expected}",
                entry.account_id,
                entry.score,
                place + 1
            ));
        }
        if entry.time != finish.map_or(-1, |time| time as i32) {
            return Err(format!(
                "{} has the time {} instead of {finish:?}",
                entry.account_id, entry.time
            ));
        }
    }
    Ok(())
}
