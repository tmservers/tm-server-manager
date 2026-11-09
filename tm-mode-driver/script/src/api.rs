//! What scripts can use: the standings, the races of a round, events, commands and what the host knows.
//!
//! Scripts are part of competitions, so like for leaderboard scripts the meaning of
//! everything registered here must never change. Changes have to keep existing
//! scripts working.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use rhai::{Array, Dynamic, Engine, EvalAltResult, INT, Map};
use tm_mode_driver_sdk::{
    self as sdk, CanSpawn, Command, CountedRace, Entry, FinishTimeout, Label, Laps, LogLevel,
    RaceRules, RoundFormat, Status, Tone, WarmUp, race_order,
};

pub(crate) type ScriptResult<T> = Result<T, Box<EvalAltResult>>;

/// The standings of the match. Scripts change them in place, like built-in modes do.
#[derive(Clone)]
pub(crate) struct Standings(Rc<RefCell<sdk::Standings>>);

impl Standings {
    pub(crate) fn new(standings: sdk::Standings) -> Self {
        Standings(Rc::new(RefCell::new(standings)))
    }

    /// The standings as the script left them.
    pub(crate) fn get(&self) -> sdk::Standings {
        self.0.borrow().clone()
    }

    fn player(&self, account_id: &str) -> ScriptResult<std::cell::RefMut<'_, Entry>> {
        std::cell::RefMut::filter_map(self.0.borrow_mut(), |standings| {
            standings.player_mut(account_id)
        })
        .map_err(|_| format!("{account_id} is not part of the standings, join them first.").into())
    }
}

/// What a script can ask the host for, as it was when the call started.
#[derive(Clone)]
pub(crate) struct Context {
    pub(crate) settings: Map,
    /// Everyone the call is about: the players of the standings and of the round.
    pub(crate) users: Rc<BTreeMap<String, sdk::User>>,
    pub(crate) map_count: u32,
    /// What the script logs, handed to the host after the call.
    pub(crate) logs: Rc<RefCell<Vec<(LogLevel, String)>>>,
}

/// The races of a round.
#[derive(Clone)]
pub(crate) struct Round(pub(crate) Rc<sdk::RoundEnd>);

#[derive(Clone)]
struct Racer(sdk::Racer);

#[derive(Clone)]
struct Race(sdk::Race);

/// What happened, see [`sdk::Event`].
#[derive(Clone)]
pub(crate) struct Event {
    kind: &'static str,
    account_id: Option<String>,
    round: Option<Round>,
}

impl From<sdk::Event> for Event {
    fn from(event: sdk::Event) -> Self {
        let (kind, account_id, round) = match event {
            sdk::Event::MatchStart => ("match_start", None, None),
            sdk::Event::MapStart => ("map_start", None, None),
            sdk::Event::RoundStart => ("round_start", None, None),
            sdk::Event::RaceStart(account_id) => ("race_start", Some(account_id), None),
            sdk::Event::RoundEnd(round) => ("round_end", None, Some(Round(Rc::new(round)))),
            sdk::Event::MapEnd => ("map_end", None, None),
            sdk::Event::MatchEnd => ("match_end", None, None),
            sdk::Event::SettingsChanged => ("settings_changed", None, None),
        };
        Event {
            kind,
            account_id,
            round,
        }
    }
}

fn int(name: &str, value: u32) -> ScriptResult<INT> {
    INT::try_from(value).map_err(|_| format!("{value} is too big for {name}.").into())
}

fn unsigned(name: &str, value: INT) -> ScriptResult<u32> {
    u32::try_from(value).map_err(|_| format!("{name} cannot be {value}.").into())
}

fn optional(name: &str, value: Option<u32>) -> ScriptResult<Dynamic> {
    match value {
        Some(value) => Ok(Dynamic::from_int(int(name, value)?)),
        None => Ok(Dynamic::UNIT),
    }
}

/// Which race of a round counts for a player.
fn counted(name: &str) -> ScriptResult<CountedRace> {
    match name {
        "last" => Ok(CountedRace::Last),
        "best" => Ok(CountedRace::Best),
        other => {
            Err(format!("The race that counts is \"last\" or \"best\", not \"{other}\".").into())
        }
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Playing => "playing",
        Status::Eliminated => "eliminated",
        Status::Spectator => "spectator",
    }
}

fn status(name: &str) -> ScriptResult<Status> {
    match name {
        "playing" => Ok(Status::Playing),
        "eliminated" => Ok(Status::Eliminated),
        "spectator" => Ok(Status::Spectator),
        other => Err(format!(
            "A status is \"playing\", \"eliminated\" or \"spectator\", not \"{other}\"."
        )
        .into()),
    }
}

fn tone_name(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "neutral",
        Tone::Positive => "positive",
        Tone::Negative => "negative",
    }
}

fn tone(name: &str) -> ScriptResult<Tone> {
    match name {
        "neutral" => Ok(Tone::Neutral),
        "positive" => Ok(Tone::Positive),
        "negative" => Ok(Tone::Negative),
        other => Err(format!(
            "A tone is \"neutral\", \"positive\" or \"negative\", not \"{other}\"."
        )
        .into()),
    }
}

fn entries(entries: &[Entry]) -> Array {
    entries.iter().cloned().map(Dynamic::from).collect()
}

fn texts(texts: impl IntoIterator<Item = String>) -> Array {
    texts.into_iter().map(Dynamic::from).collect()
}

/// The accounts of a list a script passed.
fn accounts(list: &Array) -> ScriptResult<Vec<String>> {
    list.iter()
        .map(|account| {
            account
                .clone()
                .into_string()
                .map_err(|kind| format!("Expected an account but got a {kind}.").into())
        })
        .collect()
}

pub(crate) fn register(engine: &mut Engine) {
    register_standings(engine);
    register_entry(engine);
    register_races(engine);
    register_event(engine);
    register_context(engine);
    register_commands(engine);
}

fn register_standings(engine: &mut Engine) {
    engine
        .register_type_with_name::<Standings>("Standings")
        .register_get("entries", |s: &mut Standings| {
            entries(&s.0.borrow().entries)
        })
        .register_get("round", |s: &mut Standings| entries(&s.0.borrow().round))
        .register_get("info", |s: &mut Standings| s.0.borrow().info.clone())
        .register_set("info", |s: &mut Standings, info: &str| {
            s.0.borrow_mut().info = info.into();
        })
        .register_get("rounds_on_map", |s: &mut Standings| {
            int("rounds_on_map", s.0.borrow().rounds_on_map)
        })
        .register_set("rounds_on_map", |s: &mut Standings, rounds: INT| {
            s.0.borrow_mut().rounds_on_map = unsigned("rounds_on_map", rounds)?;
            ScriptResult::Ok(())
        })
        .register_get("rounds_played", |s: &mut Standings| {
            int("rounds_played", s.0.borrow().rounds_played)
        })
        .register_get("maps_played", |s: &mut Standings| {
            int("maps_played", s.0.borrow().maps_played)
        })
        // The entry of a player in the match, nothing if the player is not part of it.
        .register_fn("player", |s: &mut Standings, account_id: &str| {
            match s.0.borrow().player(account_id) {
                Some(entry) => Dynamic::from(entry.clone()),
                None => Dynamic::UNIT,
            }
        })
        // A player new to the standings starts playing without points.
        .register_fn("join", |s: &mut Standings, account_id: &str| {
            s.0.borrow_mut().join(account_id).clone()
        })
        // Adds points to the match and to the round of a player.
        .register_fn(
            "award",
            |s: &mut Standings, account_id: &str, points: INT| {
                s.0.borrow_mut().award(account_id, points);
            },
        )
        .register_fn(
            "set_score",
            |s: &mut Standings, account_id: &str, score: INT| {
                s.player(account_id)?.score = score;
                ScriptResult::Ok(())
            },
        )
        .register_fn(
            "set_status",
            |s: &mut Standings, account_id: &str, name: &str| {
                s.player(account_id)?.status = status(name)?;
                ScriptResult::Ok(())
            },
        )
        .register_fn(
            "set_position",
            |s: &mut Standings, account_id: &str, position: INT| {
                s.player(account_id)?.position = unsigned("position", position)?;
                ScriptResult::Ok(())
            },
        )
        .register_fn(
            "set_label",
            |s: &mut Standings, account_id: &str, text: &str, tone_name: &str| {
                s.player(account_id)?.label = Some(Label::new(text, tone(tone_name)?));
                ScriptResult::Ok(())
            },
        )
        .register_fn("clear_label", |s: &mut Standings, account_id: &str| {
            s.player(account_id)?.label = None;
            ScriptResult::Ok(())
        })
        // Everyone but spectators.
        .register_fn("participants", |s: &mut Standings| -> Array {
            s.0.borrow()
                .participants()
                .cloned()
                .map(Dynamic::from)
                .collect()
        })
        // Participants who are not eliminated.
        .register_fn("in_match", |s: &mut Standings| -> Array {
            s.0.borrow()
                .in_match()
                .cloned()
                .map(Dynamic::from)
                .collect()
        })
        // Ranks by points, players with the same points by their race in the round, which never ties.
        .register_fn("rank", |s: &mut Standings, round: Round, race: &str| {
            s.0.borrow_mut().rank(&round.0, counted(race)?);
            ScriptResult::Ok(())
        })
        // Places the round that ends by the race, call it once the points are awarded.
        .register_fn(
            "place_round",
            |s: &mut Standings, round: Round, race: &str| {
                s.0.borrow_mut().place_round(&round.0, counted(race)?);
                ScriptResult::Ok(())
            },
        )
        // The round placed with these points by account, without changing the standings.
        .register_fn(
            "placed",
            |s: &mut Standings, round: Round, race: &str, points: Map| -> ScriptResult<Array> {
                let placed = s.0.borrow().placed(&round.0, counted(race)?, |account_id| {
                    points
                        .get(account_id)
                        .and_then(|points| points.as_int().ok())
                        .unwrap_or(0)
                });
                Ok(entries(&placed))
            },
        )
        // The player ranked first wins the match.
        .register_fn("crown_leaders", |s: &mut Standings| {
            s.0.borrow_mut().crown_leaders();
        })
        // Keeps everyone who does not take part from spawning and shows them as spectators.
        .register_fn("block_others", |s: &mut Standings, round: Round| -> Array {
            s.0.borrow_mut()
                .block_others(&round.0)
                .into_iter()
                .map(Dynamic::from)
                .collect()
        });
}

fn register_entry(engine: &mut Engine) {
    engine
        .register_type_with_name::<Entry>("Entry")
        .register_get("account_id", |e: &mut Entry| e.account_id.clone())
        .register_get("position", |e: &mut Entry| int("position", e.position))
        .register_set("position", |e: &mut Entry, position: INT| {
            e.position = unsigned("position", position)?;
            ScriptResult::Ok(())
        })
        .register_get("score", |e: &mut Entry| e.score)
        .register_set("score", |e: &mut Entry, score: INT| e.score = score)
        .register_get("time", |e: &mut Entry| optional("time", e.time))
        .register_set("time", |e: &mut Entry, time: INT| {
            e.time = Some(unsigned("time", time)?);
            ScriptResult::Ok(())
        })
        .register_get("status", |e: &mut Entry| status_name(e.status))
        .register_get("is_in_match", |e: &mut Entry| e.is_in_match())
        // The text shown in place of the points, nothing without a label.
        .register_get("label", |e: &mut Entry| match &e.label {
            Some(label) => Dynamic::from(label.text.clone()),
            None => Dynamic::UNIT,
        })
        .register_get("tone", |e: &mut Entry| match &e.label {
            Some(label) => Dynamic::from(tone_name(label.tone)),
            None => Dynamic::UNIT,
        })
        .register_fn("set_label", |e: &mut Entry, text: &str, tone_name: &str| {
            e.label = Some(Label::new(text, tone(tone_name)?));
            ScriptResult::Ok(())
        })
        .register_fn("clear_label", |e: &mut Entry| e.label = None)
        .register_fn("to_string", |e: &mut Entry| format!("{e:?}"))
        .register_fn("to_debug", |e: &mut Entry| format!("{e:?}"));
}

fn register_races(engine: &mut Engine) {
    engine
        .register_type_with_name::<Round>("Round")
        // The round does not count, e.g. it was forced to end.
        .register_get("invalid", |r: &mut Round| r.0.invalid)
        // Everyone who started a race in the round in the order of their first start.
        .register_get("racers", |r: &mut Round| -> Array {
            r.0.racers
                .iter()
                .cloned()
                .map(|racer| Dynamic::from(Racer(racer)))
                .collect()
        })
        // The accounts on the server when the round ended, racers or not.
        .register_get("present", |r: &mut Round| {
            texts(r.0.present.iter().cloned())
        })
        .register_fn("racer", |r: &mut Round, account_id: &str| {
            match r.0.racer(account_id) {
                Some(racer) => Dynamic::from(Racer(racer.clone())),
                None => Dynamic::UNIT,
            }
        })
        // The accounts of the racers with a race that counts, in the order of their first start.
        .register_fn(
            "finishers",
            |r: &mut Round, race: &str| -> ScriptResult<Array> {
                let counted = counted(race)?;
                Ok(texts(
                    r.0.racers
                        .iter()
                        .filter(|racer| counted.time(racer).is_some())
                        .map(|racer| racer.account_id.clone()),
                ))
            },
        )
        // The round with only the racers who take part in the match.
        .register_fn("taking_part", |r: &mut Round, standings: Standings| {
            Round(Rc::new(r.0.taking_part(&standings.0.borrow())))
        });

    engine
        .register_type_with_name::<Racer>("Racer")
        .register_get("account_id", |r: &mut Racer| r.0.account_id.clone())
        // Every race of the player in the round in the order they started them.
        .register_get("races", |r: &mut Racer| -> Array {
            r.0.races
                .iter()
                .cloned()
                .map(|race| Dynamic::from(Race(race)))
                .collect()
        })
        // The time of the race that counts, nothing without a finish.
        .register_fn("time", |r: &mut Racer, race: &str| {
            optional("time", counted(race)?.time(&r.0))
        });

    engine
        .register_type_with_name::<Race>("Race")
        // The race times at the checkpoints passed, the finish included.
        .register_get("checkpoints", |r: &mut Race| -> ScriptResult<Array> {
            r.0.checkpoints
                .iter()
                .map(|time| int("checkpoint", *time).map(Dynamic::from_int))
                .collect()
        })
        .register_get("finished", |r: &mut Race| r.0.finished)
        .register_get("laps", |r: &mut Race| int("laps", r.0.laps))
        // The race time at the finish, nothing without a finish.
        .register_get("time", |r: &mut Race| optional("time", r.0.time()));

    // The accounts by their races in the round, the best first and without ties:
    // by the time at the last checkpoint, then at the one before and so on, finally by account.
    engine.register_fn(
        "race_order",
        |players: Array, round: Round, race: &str| -> ScriptResult<Array> {
            let players = accounts(&players)?;
            Ok(texts(race_order(
                players.iter().map(String::as_str),
                &round.0,
                counted(race)?,
            )))
        },
    );
}

fn register_event(engine: &mut Engine) {
    engine
        .register_type_with_name::<Event>("Event")
        .register_get("kind", |e: &mut Event| e.kind)
        // The account of the player who started their first race of the round.
        .register_get("account_id", |e: &mut Event| -> ScriptResult<String> {
            e.account_id
                .clone()
                .ok_or_else(|| format!("A {} event has no account.", e.kind).into())
        })
        // The races of the round which ended.
        .register_get("round", |e: &mut Event| -> ScriptResult<Round> {
            e.round
                .clone()
                .ok_or_else(|| format!("A {} event has no round.", e.kind).into())
        });
}

fn register_context(engine: &mut Engine) {
    engine
        .register_type_with_name::<Context>("Context")
        .register_get("settings", |cx: &mut Context| cx.settings.clone())
        // Maps in the playlist of the match, 0 without one.
        .register_get("map_count", |cx: &mut Context| {
            int("map_count", cx.map_count)
        })
        // The name of a player, the account while the host does not know it.
        .register_fn("name", |cx: &mut Context, account_id: &str| -> String {
            cx.users
                .get(account_id)
                .map(|user| user.name.clone())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| account_id.into())
        })
        // What the host knows about a player, nothing for a player it does not know.
        .register_fn("user", |cx: &mut Context, account_id: &str| {
            let Some(user) = cx.users.get(account_id) else {
                return Dynamic::UNIT;
            };
            let mut map = Map::new();
            map.insert("account_id".into(), user.account_id.clone().into());
            map.insert("name".into(), user.name.clone().into());
            map.insert("club_tag".into(), user.club_tag.clone().into());
            map.insert("zone".into(), user.zone.clone().into());
            Dynamic::from_map(map)
        })
        // The maps of a match by a `maps_per_match` setting: the playlist once without
        // the setting, one map without a playlist. 0 means no limit.
        .register_fn(
            "maps_per_match",
            |cx: &mut Context, setting: Dynamic| -> ScriptResult<INT> {
                if setting.is_unit() {
                    return int("map_count", cx.map_count.max(1));
                }
                let maps = setting
                    .as_int()
                    .map_err(|kind| format!("maps_per_match has to be a number, not a {kind}."))?;
                unsigned("maps_per_match", maps)?;
                Ok(maps)
            },
        )
        .register_fn("log", |cx: &mut Context, message: &str| {
            cx.logs.borrow_mut().push((LogLevel::Info, message.into()));
        })
        .register_fn("warn", |cx: &mut Context, message: &str| {
            cx.logs.borrow_mut().push((LogLevel::Warn, message.into()));
        });
}

fn register_commands(engine: &mut Engine) {
    engine
        .register_type_with_name::<Command>("Command")
        .register_fn("to_string", |c: &mut Command| format!("{c:?}"))
        .register_fn("to_debug", |c: &mut Command| format!("{c:?}"))
        // Rounds of one try with a finish timeout: "automatic", "disabled",
        // milliseconds or `#{ milliseconds: 5000 }` like in the settings of the built-in modes.
        .register_fn(
            "single_attempt",
            |timeout: Dynamic| -> ScriptResult<Command> {
                Ok(Command::SetRoundFormat(RoundFormat::SingleAttempt(
                    finish_timeout(&timeout)?,
                )))
            },
        )
        // Rounds in which players start again until the time limit in milliseconds.
        .register_fn("time_attack", |time_limit: INT| -> ScriptResult<Command> {
            Ok(Command::SetRoundFormat(RoundFormat::TimeAttack(unsigned(
                "time limit",
                time_limit,
            )?)))
        })
        // The warmup before the first round of the map: `#{ rounds, duration_ms, timeout_ms }`.
        .register_fn("warm_up", |warm_up: Map| -> ScriptResult<Command> {
            let number = |name: &str| -> ScriptResult<Option<u32>> {
                match warm_up.get(name) {
                    None => Ok(None),
                    Some(value) if value.is_unit() => Ok(None),
                    Some(value) => {
                        let value = value.as_int().map_err(|kind| {
                            format!("{name} of a warmup has to be a number, not a {kind}.")
                        })?;
                        Ok(Some(unsigned(name, value)?))
                    }
                }
            };
            Ok(Command::SetWarmUp(WarmUp {
                rounds: number("rounds")?.unwrap_or(0),
                duration_ms: number("duration_ms")?.unwrap_or(0),
                timeout_ms: number("timeout_ms")?,
            }))
        })
        // The laps of the races, "map", "infinite" or a number, and whether players can give up.
        .register_fn(
            "race_rules",
            |laps: Dynamic, give_up: bool| -> ScriptResult<Command> {
                let laps = if let Ok(count) = laps.as_int() {
                    Laps::Count(unsigned("laps", count)?)
                } else {
                    match laps.into_immutable_string().as_deref() {
                        Ok("map") => Laps::Map,
                        Ok("infinite") => Laps::Infinite,
                        _ => return Err("Laps are \"map\", \"infinite\" or a number.".into()),
                    }
                };
                Ok(Command::SetRaceRules(RaceRules { laps, give_up }))
            },
        )
        .register_fn("can_spawn", |account_id: &str, can_spawn: bool| {
            Command::SetCanSpawn(CanSpawn {
                account_id: account_id.into(),
                can_spawn,
            })
        })
        .register_fn("end_map", || Command::EndMap)
        .register_fn("end_match", || Command::EndMatch)
        // Plays this map of the playlist next, counted from 0.
        .register_fn("next_map", |index: INT| -> ScriptResult<Command> {
            Ok(Command::SetNextMap(unsigned("map", index)?))
        })
        .register_fn("net_state", |state: &str| {
            Command::SetNetState(state.into())
        })
        .register_fn("chat", |message: &str| Command::ChatMessage(message.into()))
        // A place for chat messages: 1st, 2nd, 3rd, ...
        .register_fn("ordinal", |place: INT| -> ScriptResult<String> {
            Ok(sdk::ordinal(unsigned("place", place)?))
        });
}

fn finish_timeout(timeout: &Dynamic) -> ScriptResult<FinishTimeout> {
    if let Ok(milliseconds) = timeout.as_int() {
        return Ok(FinishTimeout::Milliseconds(unsigned(
            "finish timeout",
            milliseconds,
        )?));
    }
    if timeout.is_map() {
        let map = timeout.clone().cast::<Map>();
        if let (1, Some(Ok(milliseconds))) =
            (map.len(), map.get("milliseconds").map(Dynamic::as_int))
        {
            return Ok(FinishTimeout::Milliseconds(unsigned(
                "finish timeout",
                milliseconds,
            )?));
        }
    }
    match timeout.clone().into_immutable_string().as_deref() {
        Ok("automatic") => Ok(FinishTimeout::Automatic),
        Ok("disabled") => Ok(FinishTimeout::Disabled),
        _ => Err(
            "A finish timeout is \"automatic\", \"disabled\", milliseconds or #{ milliseconds: 5000 }."
                .into(),
        ),
    }
}
