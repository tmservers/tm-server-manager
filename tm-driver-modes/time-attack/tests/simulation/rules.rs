//! Every map is one round in which the best race counts. The ranking turns into
//! points, which add up over the maps.

use std::collections::BTreeMap;

use tm_driver_mode_time_attack::Settings;
use tm_driver_sim::rules::{
    MapEnd, RoundEnd, Rules, ends_map, ends_match, playing, ranked_by_points,
};

pub struct TimeAttack {
    settings: Settings,
    totals: BTreeMap<String, i32>,
}

impl TimeAttack {
    /// The rules for a match with these settings.
    pub fn boxed(settings: &str) -> Result<Box<dyn Rules>, String> {
        let settings: Settings = tm_driver_sim::rules::settings(settings)?;
        Ok(Box::new(Self {
            settings,
            totals: BTreeMap::new(),
        }))
    }
}

impl Rules for TimeAttack {
    fn match_start(&mut self) {
        self.totals.clear();
    }

    fn round_end(&mut self, end: &RoundEnd) -> Result<(), String> {
        if !ends_map(end.commands) {
            return Err("A time attack round does not end the map".into());
        }

        let entries = &end.round.entries;
        let finishers = entries
            .iter()
            .filter(|entry| end.truth.best_finish(&entry.account_id).is_some())
            .count();
        let mut previous = 0;
        for (place, entry) in entries.iter().enumerate() {
            let best = end.truth.best_finish(&entry.account_id);
            if best.is_some() != (place < finishers) {
                return Err(format!(
                    "{} in place {} is out of order with the finishers",
                    entry.account_id,
                    place + 1
                ));
            }
            if entry.time != best.map_or(-1, |time| time as i32) {
                return Err(format!(
                    "{} has the time {} instead of its best {best:?}",
                    entry.account_id, entry.time
                ));
            }
            if let Some(best) = best {
                if best < previous {
                    return Err(format!(
                        "{} is placed behind a slower best race",
                        entry.account_id
                    ));
                }
                previous = best;
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
            *self.totals.entry(entry.account_id.clone()).or_default() += entry.score;
        }
        for (account_id, _) in &end.truth.racers {
            if !entries.iter().any(|entry| &entry.account_id == account_id) {
                return Err(format!("{account_id} raced but has no place in the round"));
            }
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

/// Points of a finisher by place: the value of the place, the last one beyond.
fn points(repartition: &[i32], place: usize) -> i32 {
    repartition
        .get(place)
        .or(repartition.last())
        .copied()
        .unwrap_or(0)
}
