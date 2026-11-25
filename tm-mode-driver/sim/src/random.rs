//! Random values for the settings modes share, for the random settings a mode
//! hands to [`Simulator::settings`](crate::Simulator::settings).

use serde_json::{Value, json};

use crate::Rng;

/// A finish timeout setting: automatic, disabled or 1 to 15 seconds.
pub fn finish_timeout(rng: &mut Rng) -> Value {
    match rng.below(3) {
        0 => json!("automatic"),
        1 => json!("disabled"),
        _ => json!({"milliseconds": rng.between(1, 15) * 1000}),
    }
}

/// Points by place: one to six values up to `max`, at least 1, each at most the one
/// before. The first value is never 0.
pub fn falling_points(rng: &mut Rng, max: u32) -> Vec<u32> {
    let mut points = rng.between(1, max.max(1));
    (0..rng.between(1, 6))
        .map(|_| {
            let value = points;
            points = points.saturating_sub(rng.below(4));
            value
        })
        .collect()
}

/// Points lost by place: one to eight values, each above the one before, so the
/// last one is above 0.
pub fn rising_points(rng: &mut Rng) -> Vec<u32> {
    let mut points = rng.below(4);
    (0..rng.between(1, 8))
        .map(|_| {
            points += rng.between(1, 5);
            points
        })
        .collect()
}
