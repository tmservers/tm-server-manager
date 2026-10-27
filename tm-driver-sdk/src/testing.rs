//! Runs a mode natively the way the host runs it, e.g. in unit tests.

use std::collections::BTreeMap;

use crate::{Command, Entry, Event, Mode, Standings, User};

/// A mode together with the standings the host would keep for it.
pub struct Harness<M> {
    pub mode: M,
    pub standings: Standings,
    /// The last round that counted, as the mode placed it.
    pub round: Vec<Entry>,
    /// What [`crate::map_count`] returns.
    pub map_count: u32,
    /// What [`crate::user`] returns.
    pub users: BTreeMap<String, User>,
}

impl<M: Mode> Harness<M> {
    pub fn new(settings: &str) -> Result<Self, String> {
        Ok(Self {
            mode: M::from_json(settings)?,
            standings: Standings::default(),
            round: Vec::new(),
            map_count: 0,
            users: BTreeMap::new(),
        })
    }

    /// Steps the mode after clearing the standings the way the host does.
    pub fn step(&mut self, event: Event) -> Vec<Command> {
        clear(&mut self.standings, &event);
        if let Event::MatchStart = event {
            self.round.clear();
        }
        #[cfg(not(target_arch = "wasm32"))]
        native::set(self.map_count, self.users.clone());

        let counts = matches!(&event, Event::RoundEnd(round_end) if !round_end.forced);
        let commands = self.mode.step(&mut self.standings, event);
        let round = std::mem::take(&mut self.standings.round);
        if counts {
            self.round = round;
        }
        commands
    }

    /// Panics if the player is not part of the standings.
    pub fn standing(&self, account_id: &str) -> &Entry {
        self.standings
            .player(account_id)
            .unwrap_or_else(|| panic!("{account_id} is not part of the standings"))
    }

    /// The player in the last round that counted.
    pub fn round_entry(&self, account_id: &str) -> Option<&Entry> {
        self.round
            .iter()
            .find(|entry| entry.account_id == account_id)
    }
}

/// Mirrors what the host clears before an event (`clear` in tm-wasm-driver).
fn clear(standings: &mut Standings, event: &Event) {
    standings.round.clear();
    match event {
        Event::MatchStart => *standings = Standings::default(),
        Event::MapStart => standings.rounds_on_map = 0,
        Event::RoundEnd(round_end) if !round_end.forced => {
            standings.rounds_played = standings.rounds_played.saturating_add(1);
        }
        Event::MapEnd => standings.maps_played = standings.maps_played.saturating_add(1),
        Event::RoundStart | Event::RaceStart(_) | Event::RoundEnd(_) | Event::MatchEnd => {}
    }
}

/// What the host functions return natively, set by the harness before every step.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod native {
    use std::{cell::RefCell, collections::BTreeMap};

    use crate::User;

    thread_local! {
        static HOST: RefCell<(u32, BTreeMap<String, User>)> = RefCell::default();
    }

    pub(crate) fn set(map_count: u32, users: BTreeMap<String, User>) {
        HOST.with(|host| *host.borrow_mut() = (map_count, users));
    }

    pub(crate) fn user(account_id: &str) -> Option<User> {
        HOST.with(|host| host.borrow().1.get(account_id).cloned())
    }

    pub(crate) fn map_count() -> u32 {
        HOST.with(|host| host.borrow().0)
    }
}
