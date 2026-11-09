//! What the calls into a mode cost, e.g. for benchmarks or to watch a server.
//!
//! The cost is what an engine counts for a call, e.g. the operations of a script:
//! the same on every machine, so it shows how far a call stays below its budget.
//! Built-in modes have none. Times are wall clock and include the host: building
//! the arguments, the call and publishing what changed. Neither is part of what a
//! replay rebuilds.

use std::{collections::BTreeMap, time::Duration};

/// The calls of a driver so far by kind, e.g. `round-end` or `live-round`.
#[derive(Debug, Clone, Default)]
pub struct DriverStats {
    pub calls: BTreeMap<&'static str, CallStats>,
}

/// The calls of one kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallStats {
    pub count: u64,
    /// All of them together.
    pub cost: u64,
    pub max_cost: u64,
    /// Building the arguments from what the host knows, e.g. the races of a round.
    pub prepare: Duration,
    /// The calls, copying the arguments into the mode and the result back included.
    pub call: Duration,
    pub max_call: Duration,
    /// Publishing what changed after the calls.
    pub publish: Duration,
    /// The most racers and checkpoints of all their races a call got.
    pub max_racers: u64,
    pub max_checkpoints: u64,
}

impl CallStats {
    pub fn mean_cost(&self) -> u64 {
        self.cost.checked_div(self.count).unwrap_or(0)
    }

    pub fn mean_call(&self) -> Duration {
        match u32::try_from(self.count) {
            Ok(count) if count > 0 => self.call / count,
            _ => Duration::ZERO,
        }
    }
}

/// One call as the host measured it.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Measured {
    pub cost: u64,
    pub prepare: Duration,
    pub call: Duration,
    pub publish: Duration,
    pub racers: u64,
    pub checkpoints: u64,
}

impl DriverStats {
    pub(crate) fn record(&mut self, kind: &'static str, measured: Measured) {
        let stats = self.calls.entry(kind).or_default();
        stats.count += 1;
        stats.cost = stats.cost.saturating_add(measured.cost);
        stats.max_cost = stats.max_cost.max(measured.cost);
        stats.prepare += measured.prepare;
        stats.call += measured.call;
        stats.max_call = stats.max_call.max(measured.call);
        stats.publish += measured.publish;
        stats.max_racers = stats.max_racers.max(measured.racers);
        stats.max_checkpoints = stats.max_checkpoints.max(measured.checkpoints);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_add_up_by_kind() {
        let mut stats = DriverStats::default();
        for cost in [100, 300] {
            stats.record(
                "round-end",
                Measured {
                    cost,
                    call: Duration::from_micros(cost),
                    racers: cost / 100,
                    ..Measured::default()
                },
            );
        }
        let round_end = &stats.calls["round-end"];
        assert_eq!(
            (round_end.count, round_end.max_cost, round_end.mean_cost()),
            (2, 300, 200)
        );
        assert_eq!(round_end.mean_call(), Duration::from_micros(200));
        assert_eq!(round_end.max_racers, 3);
    }
}
