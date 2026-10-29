//! Runs a mode natively the way the host runs it, e.g. in unit tests.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::{Command, Entry, Event, Mode, RoundEnd, Standings, User, reload::Configured};

/// A mode together with the standings the host would keep for it.
pub struct Harness<M> {
    configured: Configured<M>,
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
            configured: Configured::new(settings)?,
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

        let counts = matches!(&event, Event::RoundEnd(round_end) if !round_end.invalid);
        let commands = self.configured.mode.step(&mut self.standings, event);
        let round = std::mem::take(&mut self.standings.round);
        if counts {
            self.round = round;
        }
        commands
    }

    /// The round as it stands with the races so far, like the host shows it while
    /// the round runs. Changes nothing.
    pub fn live_round(&self, races: RoundEnd) -> Vec<Entry> {
        #[cfg(not(target_arch = "wasm32"))]
        native::set(self.map_count, self.users.clone());
        self.configured.mode.live_round(&self.standings, &races)
    }

    /// The mode with the settings it runs with.
    pub fn mode(&self) -> &M {
        &self.configured.mode
    }

    /// New settings during the match, like the host gives them to the mode once no
    /// round runs: the mode takes them and gets [`Event::SettingsChanged`]. Settings
    /// the mode cannot change during a match are rejected and change nothing.
    pub fn change_settings(&mut self, settings: &str) -> Result<Vec<Command>, String> {
        self.configured = self.configured.change(settings)?;
        Ok(self.step(Event::SettingsChanged))
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
        Event::RoundEnd(round_end) if !round_end.invalid => {
            standings.rounds_played = standings.rounds_played.saturating_add(1);
        }
        Event::MapEnd => standings.maps_played = standings.maps_played.saturating_add(1),
        Event::RoundStart
        | Event::RaceStart(_)
        | Event::RoundEnd(_)
        | Event::MatchEnd
        | Event::SettingsChanged => {}
    }
}

/// Panics unless every name [`Mode::RELOADABLE`] lists is a setting of the mode,
/// e.g. after one was renamed.
pub fn assert_reloadable_settings_exist<M: Mode>() {
    let configured = Configured::<M>::new("{}").expect("The default settings are valid");
    let Value::Object(settings) = &configured.settings else {
        panic!("The settings of {} are no json object", M::NAME);
    };
    for name in M::RELOADABLE {
        assert!(
            settings.contains_key(*name),
            "{name} is no setting of {}",
            M::NAME
        );
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
