//! Runs a mode natively the way the host runs it, e.g. in unit tests.

use crate::{Command, Event, Mode, Standing, Standings};

/// A mode together with the standings the host would keep for it.
pub struct Harness<M> {
    pub mode: M,
    pub standings: Standings,
}

impl<M: Mode> Harness<M> {
    pub fn new(settings: &str) -> Result<Self, String> {
        Ok(Self {
            mode: M::init(settings)?,
            standings: Standings::default(),
        })
    }

    /// Steps the mode after clearing the standings the way the host does.
    pub fn step(&mut self, event: Event) -> Vec<Command> {
        clear(&mut self.standings, &event);
        self.mode.step(&mut self.standings, event)
    }

    /// Panics if the player is not part of the standings.
    pub fn standing(&self, account_id: &str) -> &Standing {
        self.standings
            .player(account_id)
            .unwrap_or_else(|| panic!("{account_id} is not part of the standings"))
    }
}

/// Mirrors what the host clears before an event (`clear` in tm-wasm-driver).
fn clear(standings: &mut Standings, event: &Event) {
    match event {
        Event::MatchStart => *standings = Standings::default(),
        Event::MapStart => {
            standings.rounds_on_map = 0;
            for player in &mut standings.players {
                player.map_points = 0;
            }
        }
        Event::RoundEnd(_) => {
            for player in &mut standings.players {
                player.round_points = 0;
            }
        }
        Event::MapEnd => standings.maps_played = standings.maps_played.saturating_add(1),
        Event::RoundStart | Event::RaceStart(_) | Event::MatchEnd => {}
    }
}
