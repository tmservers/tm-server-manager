//! Leaderboard steps written as Rhai scripts.
//!
//! A script defines `fn step(cx, entries)` and returns the new entries, everything else of it is ignored.
//! Scripts can only read the database through `cx`, which only does point lookups,
//! so a view evaluating them only depends on the rows they read.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use rhai::{
    AST, Array, CallFnOptions, Dynamic, Engine, EvalAltResult, FnPtr, ImmutableString, Map,
    NativeCallContext, Scope,
    packages::{BasicArrayPackage, BasicMapPackage, CorePackage, LogicPackage, Package},
};
use spacetimedb::SpacetimeType;

use crate::{
    competition::{CompetitionPermissionsV1, node::NodeHandle},
    leaderboard::{
        LbEntry, LbEntryKind, LbParams, LbParticipant, LbSettingsV3, LbStep,
        context::LbContext,
        error::LbError,
        modes::{match_standings, match_team_standings},
        shape::{LbOrigins, LbShape},
    },
};

/// The function a script has to define, it gets the context and the entries.
const STEP_HOOK: &str = "step";

/// Stops scripts which would run for too long, e.g. because they never end.
const MAX_OPERATIONS: u64 = 5_000_000;

/// Compiled scripts are kept up to this many, then they get compiled again.
const MAX_COMPILED: usize = 64;

/// Runs a script on the entries.
///
/// What a script does is part of the competition, so like for every other step its meaning must never change.
/// Changes to the registered API have to keep existing scripts working.
#[derive(Debug, SpacetimeType, Clone, PartialEq, Eq)]
pub(crate) struct LbScriptSettings {
    pub(crate) source: String,
    pub(crate) output: LbScriptOutput,
}

/// What a script returns, every run is checked against it so the steps after it can be checked on save.
#[derive(Debug, SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LbScriptOutput {
    /// Rounds of players, which can only come from the entries the script got.
    Rounds,
    /// Standings of players.
    Standings,
    /// Standings of teams.
    TeamStandings,
}

impl LbScriptOutput {
    fn check(self, entry: &LbEntry) -> Result<(), LbError> {
        let (kind, teams) = match self {
            LbScriptOutput::Rounds => (LbEntryKind::Round, false),
            LbScriptOutput::Standings => (LbEntryKind::Standing, false),
            LbScriptOutput::TeamStandings => (LbEntryKind::Standing, true),
        };
        let is_team = matches!(entry.participant(), LbParticipant::Team(_));
        if entry.kind != kind || is_team != teams {
            return Err(LbError::new(format!(
                "The script returned {entry:?} but has to return {self:?}."
            )));
        }
        Ok(())
    }
}

impl LbStep for LbScriptSettings {
    fn apply(&self, cx: &LbContext, entries: Vec<LbEntry>) -> Result<Vec<LbEntry>, LbError> {
        let ast = compiled(&self.source)?;
        let entries: Array = entries.into_iter().map(Dynamic::from).collect();

        let returned = ENGINE
            .with(|engine| {
                engine.call_fn_with_options::<Array>(
                    // Only the functions of the script are used.
                    CallFnOptions::new().eval_ast(false),
                    &mut Scope::new(),
                    &ast,
                    STEP_HOOK,
                    (cx.clone(), entries),
                )
            })
            .map_err(|error| LbError::new(format!("The script failed: {error}")))?;

        returned
            .into_iter()
            .map(|value| {
                let type_name = value.type_name();
                let entry = value.try_cast::<LbEntry>().ok_or_else(|| {
                    LbError::new(format!(
                        "The script returned a {type_name} instead of an entry."
                    ))
                })?;
                self.output.check(&entry)?;
                Ok(entry)
            })
            .collect()
    }

    fn check(&self, origin: NodeHandle, input: LbShape) -> Result<LbShape, LbError> {
        compiled(&self.source)?;
        if input.is_empty() {
            return Ok(input);
        }

        let rounds = self.output == LbScriptOutput::Rounds;
        if rounds && !input.rounds {
            return Err(LbError::new(
                "The script can only return rounds it got, but it gets none.",
            ));
        }
        // Standings can also be created by the script, they then belong to the origin.
        let origins = if rounds {
            input.origins
        } else {
            input
                .combine(LbShape {
                    origins: LbOrigins::One(origin),
                    ..LbShape::default()
                })
                .origins
        };
        let teams = self.output == LbScriptOutput::TeamStandings;
        Ok(LbShape {
            rounds,
            standings: !rounds,
            origins,
            maps: input.maps,
            players: !teams,
            teams,
        })
    }
}

/// The permission needed on top to change the steps from `old` to `new`.
pub(crate) fn script_permission(
    old: &[LbSettingsV3],
    new: &[LbSettingsV3],
) -> CompetitionPermissionsV1 {
    if scripts_changed(old, new) {
        CompetitionPermissionsV1::SCRIPT_EDIT
    } else {
        CompetitionPermissionsV1::NONE
    }
}

/// Whether the scripts of the steps differ, the other steps can change without it.
fn scripts_changed(old: &[LbSettingsV3], new: &[LbSettingsV3]) -> bool {
    let scripts = |steps: &[LbSettingsV3]| -> Vec<LbScriptSettings> {
        steps
            .iter()
            .filter_map(|step| match step {
                LbSettingsV3::Script(script) => Some(script.clone()),
                _ => None,
            })
            .collect()
    };
    scripts(old) != scripts(new)
}

thread_local! {
    static ENGINE: Engine = engine();
    static COMPILED: RefCell<HashMap<String, Rc<AST>>> = RefCell::default();
}

/// The script compiled, it has to define the hook.
fn compiled(source: &str) -> Result<Rc<AST>, LbError> {
    if let Some(ast) = COMPILED.with_borrow(|compiled| compiled.get(source).cloned()) {
        return Ok(ast);
    }

    let ast = ENGINE
        .with(|engine| engine.compile(source))
        .map_err(|error| LbError::new(format!("The script does not compile: {error}")))?;
    if !ast
        .iter_functions()
        .any(|f| f.name == STEP_HOOK && f.params.len() == 2)
    {
        return Err(LbError::new(format!(
            "The script has to define `fn {STEP_HOOK}(cx, entries)`."
        )));
    }

    let ast = Rc::new(ast);
    COMPILED.with_borrow_mut(|compiled| {
        if compiled.len() >= MAX_COMPILED {
            compiled.clear();
        }
        compiled.insert(source.to_owned(), ast.clone());
    });
    Ok(ast)
}

type ScriptResult<T> = Result<T, Box<EvalAltResult>>;

fn engine() -> Engine {
    let mut engine = Engine::new_raw();
    engine.register_global_module(CorePackage::new().as_shared_module());
    engine.register_global_module(LogicPackage::new().as_shared_module());
    engine.register_global_module(BasicArrayPackage::new().as_shared_module());
    engine.register_global_module(BasicMapPackage::new().as_shared_module());

    engine
        .set_max_operations(MAX_OPERATIONS)
        .set_max_call_levels(32)
        .set_max_expr_depths(64, 32)
        .set_max_string_size(4096)
        .set_max_array_size(100_000)
        .set_max_map_size(1_000)
        // Typos are errors when compiling instead of silently being nothing.
        .set_strict_variables(true)
        .set_fail_on_invalid_map_property(true);

    engine.on_print(|text| log::info!("Leaderboard script: {text}"));
    engine.on_debug(|text, _, position| log::info!("Leaderboard script {position}: {text}"));

    register_entry(&mut engine);
    register_participant(&mut engine);
    register_node(&mut engine);
    register_context(&mut engine);
    register_combinators(&mut engine);
    engine
}

fn int<T: TryFrom<i64>>(name: &str, value: i64) -> ScriptResult<T> {
    T::try_from(value).map_err(|_| format!("{value} is out of range for {name}.").into())
}

fn register_entry(engine: &mut Engine) {
    engine
        .register_type_with_name::<LbEntry>("Entry")
        .register_get("user_id", |e: &mut LbEntry| e.user_id as i64)
        .register_get("team_id", |e: &mut LbEntry| e.team_id as i64)
        .register_get("map_id", |e: &mut LbEntry| e.map_id as i64)
        .register_get("participant", |e: &mut LbEntry| e.participant())
        .register_get("node", |e: &mut LbEntry| e.get_node())
        .register_get("is_round", |e: &mut LbEntry| e.kind == LbEntryKind::Round)
        .register_get("position", |e: &mut LbEntry| e.position as i64)
        .register_set("position", |e: &mut LbEntry, v: i64| {
            e.position = int("position", v)?;
            ScriptResult::Ok(())
        })
        .register_get("round", |e: &mut LbEntry| e.round as i64)
        .register_set("round", |e: &mut LbEntry, v: i64| {
            e.round = int("round", v)?;
            ScriptResult::Ok(())
        })
        .register_get("score", |e: &mut LbEntry| e.score as i64)
        .register_set("score", |e: &mut LbEntry, v: i64| {
            e.score = int("score", v)?;
            ScriptResult::Ok(())
        })
        .register_get("time", |e: &mut LbEntry| e.time as i64)
        .register_set("time", |e: &mut LbEntry, v: i64| {
            e.time = int("time", v)?;
            ScriptResult::Ok(())
        })
        .register_fn("to_string", |e: &mut LbEntry| format!("{e:?}"))
        .register_fn("to_debug", |e: &mut LbEntry| format!("{e:?}"));
}

fn register_participant(engine: &mut Engine) {
    fn id(value: i64) -> ScriptResult<u32> {
        match int::<u32>("id", value)? {
            0 => Err("An id cannot be 0.".into()),
            id => Ok(id),
        }
    }

    engine
        .register_type_with_name::<LbParticipant>("Participant")
        .register_fn("player", |user_id: i64| {
            ScriptResult::Ok(LbParticipant::Player(id(user_id)?))
        })
        .register_fn("team", |team_id: i64| {
            ScriptResult::Ok(LbParticipant::Team(id(team_id)?))
        })
        .register_get("id", |p: &mut LbParticipant| match *p {
            LbParticipant::Player(id) | LbParticipant::Team(id) => id as i64,
        })
        .register_get("is_team", |p: &mut LbParticipant| {
            matches!(p, LbParticipant::Team(_))
        })
        .register_fn("==", |a: LbParticipant, b: LbParticipant| a == b)
        .register_fn("!=", |a: LbParticipant, b: LbParticipant| a != b)
        .register_fn("to_string", |p: &mut LbParticipant| format!("{p:?}"))
        .register_fn("to_debug", |p: &mut LbParticipant| format!("{p:?}"));
}

fn register_node(engine: &mut Engine) {
    engine
        .register_type_with_name::<NodeHandle>("Node")
        .register_get("id", |n: &mut NodeHandle| n.split().1 as i64)
        .register_get("kind", |n: &mut NodeHandle| match n {
            NodeHandle::MatchV1(_) => "Match",
            NodeHandle::CompetitionV1(_) => "Competition",
            NodeHandle::ScheduleV1(_) => "Schedule",
            NodeHandle::ServerV1(_) => "Server",
            NodeHandle::InputV1(_) => "Input",
            NodeHandle::OutputV1(_) => "Output",
            NodeHandle::RegistrationV1(_) => "Registration",
            NodeHandle::LeaderboardV1(_) => "Leaderboard",
        })
        .register_fn("==", |a: NodeHandle, b: NodeHandle| a == b)
        .register_fn("!=", |a: NodeHandle, b: NodeHandle| a != b)
        .register_fn("to_string", |n: &mut NodeHandle| format!("{n:?}"))
        .register_fn("to_debug", |n: &mut NodeHandle| format!("{n:?}"));
}

fn entries_of(entries: Array) -> ScriptResult<Vec<LbEntry>> {
    entries
        .into_iter()
        .map(|value| {
            let type_name = value.type_name();
            value
                .try_cast::<LbEntry>()
                .ok_or_else(|| format!("Expected entries but got a {type_name}.").into())
        })
        .collect()
}

fn array_of(entries: Vec<LbEntry>) -> Array {
    entries.into_iter().map(Dynamic::from).collect()
}

fn register_context(engine: &mut Engine) {
    engine
        .register_type_with_name::<LbContext>("Context")
        .register_get("origin", |cx: &mut LbContext| cx.origin())
        // A standing without results, which belongs to the node the script runs for.
        .register_fn(
            "standing",
            |cx: &mut LbContext, participant: LbParticipant| {
                LbEntry::for_participant(participant, LbEntryKind::Standing, 0, cx.origin())
                    .set_score(0)
                    .set_time(0)
            },
        )
        .register_fn(
            "match_mode",
            |cx: &mut LbContext, node: NodeHandle| match cx.match_mode(node) {
                Some(mode) => Dynamic::from(ImmutableString::from(format!("{mode:?}"))),
                None => Dynamic::UNIT,
            },
        )
        // The built-in steps turning the rounds of every match into standings.
        .register_fn(
            "match_standings",
            |cx: &mut LbContext, entries: Array| -> ScriptResult<Array> {
                match_standings(cx, entries_of(entries)?)
                    .map(array_of)
                    .map_err(|error| error.to_string().into())
            },
        )
        .register_fn(
            "match_team_standings",
            |cx: &mut LbContext, entries: Array| -> ScriptResult<Array> {
                match_team_standings(cx, entries_of(entries)?)
                    .map(array_of)
                    .map_err(|error| error.to_string().into())
            },
        );
}

/// A value scripts group or sort entries by.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum ScriptKey {
    Unit,
    Bool(bool),
    Int(i64),
    Str(ImmutableString),
    Participant(bool, u32),
    Node(u8, u32),
    List(Vec<ScriptKey>),
}

impl ScriptKey {
    fn of(value: &Dynamic) -> ScriptResult<Self> {
        if value.is_unit() {
            Ok(ScriptKey::Unit)
        } else if let Ok(value) = value.as_bool() {
            Ok(ScriptKey::Bool(value))
        } else if let Ok(value) = value.as_int() {
            Ok(ScriptKey::Int(value))
        } else if value.is_string() {
            Ok(ScriptKey::Str(value.clone().into_immutable_string()?))
        } else if value.is::<LbParticipant>() {
            Ok(match value.clone_cast::<LbParticipant>() {
                LbParticipant::Player(id) => ScriptKey::Participant(false, id),
                LbParticipant::Team(id) => ScriptKey::Participant(true, id),
            })
        } else if value.is::<NodeHandle>() {
            let (variant, id) = value.clone_cast::<NodeHandle>().split();
            Ok(ScriptKey::Node(variant, id))
        } else if value.is_array() {
            let list = value.read_lock::<Array>().expect("checked to be an array");
            Ok(ScriptKey::List(
                list.iter()
                    .map(ScriptKey::of)
                    .collect::<ScriptResult<_>>()?,
            ))
        } else {
            Err(format!("A {} cannot be grouped or sorted by.", value.type_name()).into())
        }
    }
}

fn register_combinators(engine: &mut Engine) {
    engine
        // Groups in the order their keys first appear, as `#{ key, entries }`.
        .register_fn(
            "group_by",
            |ctx: NativeCallContext, entries: &mut Array, key: FnPtr| -> ScriptResult<Array> {
                let mut indices: HashMap<ScriptKey, usize> = HashMap::new();
                let mut groups: Vec<(Dynamic, Array)> = Vec::new();
                for entry in entries.iter() {
                    let value: Dynamic = key.call_within_context(&ctx, (entry.clone(),))?;
                    match indices.get(&ScriptKey::of(&value)?) {
                        Some(&index) => groups[index].1.push(entry.clone()),
                        None => {
                            indices.insert(ScriptKey::of(&value)?, groups.len());
                            groups.push((value, vec![entry.clone()]));
                        }
                    }
                }
                Ok(groups
                    .into_iter()
                    .map(|(key, entries)| {
                        let mut group = Map::new();
                        group.insert("key".into(), key);
                        group.insert("entries".into(), entries.into());
                        group.into()
                    })
                    .collect())
            },
        )
        // The entries with the lowest key first, entries with the same key keep their order.
        .register_fn(
            "sort_by_key",
            |ctx: NativeCallContext, entries: &mut Array, key: FnPtr| -> ScriptResult<Array> {
                let mut keyed = entries
                    .iter()
                    .map(|entry| {
                        let value: Dynamic = key.call_within_context(&ctx, (entry.clone(),))?;
                        Ok((ScriptKey::of(&value)?, entry.clone()))
                    })
                    .collect::<ScriptResult<Vec<_>>>()?;
                keyed.sort_by(|(a, _), (b, _)| a.cmp(b));
                Ok(keyed.into_iter().map(|(_, entry)| entry).collect())
            },
        )
        // The positions set to the order of the entries, starting at 1.
        .register_fn("renumbered", |entries: &mut Array| -> ScriptResult<Array> {
            let mut entries = entries_of(entries.clone())?;
            for (position, entry) in (1..).zip(&mut entries) {
                entry.position = int("position", position)?;
            }
            Ok(array_of(entries))
        })
        // The entries at the places, starting at 1. Places which do not exist are skipped.
        .register_fn(
            "places",
            |entries: &mut Array, places: Array| -> ScriptResult<Array> {
                places
                    .iter()
                    .map(|place| -> ScriptResult<Option<Dynamic>> {
                        let place = place.as_int().map_err(|_| "Places have to be numbers.")?;
                        Ok(usize::try_from(place - 1)
                            .ok()
                            .and_then(|index| entries.get(index).cloned()))
                    })
                    .filter_map(Result::transpose)
                    .collect()
            },
        )
        // Sorting by it puts the best times first, times which were not driven last.
        .register_fn("time_key", |time: i64| -> ScriptResult<i64> {
            Ok(LbParams::time_key(int("time", time)?) as i64)
        });
}

#[cfg(test)]
mod tests {
    use tm_server_types::config::TmMode;

    use super::*;
    use crate::leaderboard::{
        merge::LbMergeSettingsV2,
        test_util::{MATCH, round, test_context},
    };

    const LEADERBOARD: NodeHandle = NodeHandle::LeaderboardV1(7);

    fn script(source: &str, output: LbScriptOutput) -> LbScriptSettings {
        LbScriptSettings {
            source: source.into(),
            output,
        }
    }

    fn rounds() -> Vec<LbEntry> {
        vec![
            round(1, 1, 10, 0),
            round(2, 1, 30, 0),
            round(3, 1, 20, 0),
            round(1, 2, 25, 0),
            round(3, 2, 15, 0),
        ]
    }

    #[test]
    fn a_script_can_merge_like_the_merge_step() {
        let test = test_context(&[]);
        let cx = LbContext::new(&test.anonymous_view_context(), LEADERBOARD);
        let summed = script(
            r#"
            fn step(cx, entries) {
                let standings = [];
                for group in entries.group_by(|e| e.participant) {
                    let standing = cx.standing(group.key);
                    for e in group.entries {
                        standing.score += e.score;
                    }
                    standings.push(standing);
                }
                standings.sort_by_key(|s| -s.score).renumbered()
            }
            "#,
            LbScriptOutput::Standings,
        );

        let rows = |entries: Vec<LbEntry>| -> Vec<(u32, u16, i32, NodeHandle, LbEntryKind)> {
            entries
                .iter()
                .map(|e| (e.user_id, e.position, e.score, e.get_node(), e.kind))
                .collect()
        };
        // Player 2 and 3 are tied, so this also shows both keep the order the players first appeared in.
        let merged = LbMergeSettingsV2::summate(LbParams::Score)
            .apply(&cx, rounds())
            .unwrap();
        assert_eq!(rows(summed.apply(&cx, rounds()).unwrap()), rows(merged));
    }

    #[test]
    fn scripts_can_use_the_standings_of_the_built_in_modes() {
        let test = test_context(&[(1, TmMode::Rounds)]);
        let cx = LbContext::new(&test.anonymous_view_context(), LEADERBOARD);
        let first = script(
            "fn step(cx, entries) { cx.match_standings(entries).places([1]) }",
            LbScriptOutput::Standings,
        );

        let standings = first.apply(&cx, rounds()).unwrap();

        let users: Vec<u32> = standings.iter().map(|e| e.user_id).collect();
        assert_eq!(users, vec![1]);
    }

    #[test]
    fn a_script_has_to_return_what_it_declares() {
        let test = test_context(&[]);
        let cx = LbContext::new(&test.anonymous_view_context(), LEADERBOARD);
        let unchanged = "fn step(cx, entries) { entries }";

        assert!(
            script(unchanged, LbScriptOutput::Rounds)
                .apply(&cx, rounds())
                .is_ok()
        );
        assert!(
            script(unchanged, LbScriptOutput::Standings)
                .apply(&cx, rounds())
                .is_err()
        );
    }

    #[test]
    fn a_script_which_never_ends_gets_stopped() {
        let test = test_context(&[]);
        let cx = LbContext::new(&test.anonymous_view_context(), LEADERBOARD);

        let error = script("fn step(cx, entries) { loop {} }", LbScriptOutput::Rounds)
            .apply(&cx, rounds())
            .unwrap_err();

        assert!(error.to_string().contains("Too many operations"));
    }

    #[test]
    fn scripts_which_cannot_run_are_rejected_when_saved() {
        let rounds = LbShape::rounds(MATCH);
        let check = |source: &str| script(source, LbScriptOutput::Rounds).check(MATCH, rounds);

        assert!(check("fn step(cx, entries) { entries }").is_ok());
        assert!(check("fn step(cx, entries) { entries").is_err());
        assert!(check("fn step(entries) { entries }").is_err());
        // Unknown variables are errors when compiling.
        assert!(check("fn step(cx, entries) { entires }").is_err());
    }

    #[test]
    fn the_declared_output_is_what_later_steps_get_checked_against() {
        let unchanged = "fn step(cx, entries) { entries }";
        let standings = LbShape::standings(NodeHandle::RegistrationV1(1));

        let teams = script(unchanged, LbScriptOutput::TeamStandings)
            .check(LEADERBOARD, standings)
            .unwrap();
        assert!(teams.teams && !teams.players);
        // Rounds cannot be created, so there have to be some to return.
        assert!(
            script(unchanged, LbScriptOutput::Rounds)
                .check(LEADERBOARD, standings)
                .is_err()
        );
    }

    #[test]
    fn only_changing_a_script_needs_the_script_permission() {
        let steps = |source: &str, extra: bool| {
            let mut steps = vec![LbSettingsV3::Script(script(source, LbScriptOutput::Rounds))];
            if extra {
                steps.push(LbSettingsV3::Match);
            }
            steps
        };
        let old = steps("fn step(cx, entries) { entries }", false);

        assert_eq!(
            script_permission(&old, &steps("fn step(cx, entries) { entries }", true)),
            CompetitionPermissionsV1::NONE
        );
        assert_eq!(
            script_permission(&old, &steps("fn step(cx, e) { e }", false)),
            CompetitionPermissionsV1::SCRIPT_EDIT
        );
        assert_eq!(
            script_permission(&old, &[]),
            CompetitionPermissionsV1::SCRIPT_EDIT
        );
    }
}
