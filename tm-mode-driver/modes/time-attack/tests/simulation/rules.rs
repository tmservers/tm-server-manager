//! Every map is one round in which the best race counts. The ranking turns into
//! points, which add up over the maps.

use std::collections::BTreeMap;

use tm_mode_driver_sim::rules::{
    Counted, DriverRound, DriverStandings, LiveRound, MapEnd, RoundEnd, RoundTruth, Rules,
    ends_map, ends_match, in_race_order, live_in_race_order, playing, points, racers_placed,
    ranked_by_points,
};
use tm_mode_time_attack::Settings;

pub struct TimeAttack {
    settings: Settings,
    totals: BTreeMap<String, i32>,
}

impl TimeAttack {
    /// The rules for a match with these settings.
    pub fn boxed(settings: &str) -> Result<Box<dyn Rules>, String> {
        let settings: Settings = tm_mode_driver_sim::rules::settings(settings)?;
        Ok(Box::new(Self {
            settings,
            totals: BTreeMap::new(),
        }))
    }
}

impl TimeAttack {
    /// Every player has the time of their best race and the finishers the points of
    /// its rank.
    fn check_places(&self, round: &DriverRound, truth: &RoundTruth) -> Result<(), String> {
        let finishers = round
            .entries
            .iter()
            .filter(|entry| truth.best_finish(&entry.account_id).is_some())
            .count();
        for (place, entry) in round.entries.iter().enumerate() {
            let best = truth.best_finish(&entry.account_id);
            if entry.time != best.map_or(-1, |time| time as i32) {
                return Err(format!(
                    "{} has the time {} instead of its best {best:?}",
                    entry.account_id, entry.time
                ));
            }

            let repartition = &self.settings.points_repartition;
            let expected = match best {
                None => 0,
                Some(_) if repartition.is_empty() => (finishers - place) as i32,
                Some(_) => points(repartition, place),
            };
            if entry.score != expected {
                return Err(format!(
                    "{} got {} points in place {}, not {expected}",
                    entry.account_id,
                    entry.score,
                    place + 1
                ));
            }
        }
        Ok(())
    }
}

impl Rules for TimeAttack {
    fn match_start(&mut self) {
        self.totals.clear();
    }

    fn settings_changed(&mut self, settings: &str) -> Result<(), String> {
        self.settings = tm_mode_driver_sim::rules::settings(settings)?;
        Ok(())
    }

    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        if !ends_map(end.commands) {
            return Err("A time attack round does not end the map".into());
        }

        in_race_order(end, Counted::Best)?;
        racers_placed(end)?;
        self.check_places(end.round, end.truth)?;
        for entry in &end.round.entries {
            *self.totals.entry(entry.account_id.clone()).or_default() += entry.score;
        }

        for entry in playing(end.after) {
            let total = self.totals.get(&entry.account_id).copied().unwrap_or(0);
            if entry.score != total {
                return Err(format!(
                    "{} has {} points instead of the {total} of its maps",
                    entry.account_id, entry.score
                ));
            }
        }
        ranked_by_points(end.after)
    }

    /// The best times so far have the points of their rank.
    fn live_round(&mut self, live: &LiveRound) -> Result<(), String> {
        live_in_race_order(live, Counted::Best)?;
        self.check_places(live.round, live.truth)
    }

    /// Every round ends its map, and the maps of the match end it. The current map
    /// counts even when a new playlist has fewer maps.
    fn round_limit(&self, standings: &DriverStandings) -> Option<u32> {
        let maps = self
            .settings
            .maps_per_match
            .unwrap_or(standings.map_count.max(1));
        (maps > 0).then(|| maps.max(standings.maps_played + 1))
    }

    fn map_end(&mut self, end: &MapEnd) -> Result<(), String> {
        let maps = self.settings.maps_per_match.unwrap_or(end.playlist.max(1));
        let over = maps > 0 && end.after.maps_played >= maps;
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
