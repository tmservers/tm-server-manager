use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use rhai::{AST, Engine};
use spacetimedb::{CtxDbRead, LocalReadOnly};
use tm_server_types::config::{ModeSettingsV2, TmMode};

use crate::{
    competition::node::NodeHandle,
    pipeline::steps::script,
    raw_server::config::RawServerContigRead,
    tm_match::{MatchRead, state::MatchStateRead},
};

/// What the steps of a leaderboard can read while they are evaluated for `origin`.
///
/// Steps only read what they need when they need it, so a view evaluating them only depends on those rows.
/// Reads should be point lookups, everything else makes the view depend on the whole table.
/// Nothing read from the database may be kept across evaluations, the view would then miss its changes.
///
/// One context is created per call, the nodes evaluated during it get one with [`LbContext::at`].
#[derive(Clone)]
pub(crate) struct LbContext {
    db: LocalReadOnly,
    origin: NodeHandle,
    /// Built when the first script runs and shared by every node of the call.
    script_engine: Rc<OnceCell<Engine>>,
    /// The scripts compiled during the call by their source, a script evaluated for several nodes compiles once.
    /// Compiling only depends on the source, so nothing of the database is kept.
    scripts: Rc<RefCell<HashMap<Box<str>, Rc<AST>>>>,
}

impl LbContext {
    pub(crate) fn new(ctx: &impl CtxDbRead, origin: NodeHandle) -> Self {
        LbContext {
            db: ctx.db_read_only().clone(),
            origin,
            script_engine: Rc::default(),
            scripts: Rc::default(),
        }
    }

    /// The context for evaluating `origin` during the same call.
    pub(crate) fn at(&self, origin: NodeHandle) -> Self {
        LbContext {
            origin,
            ..self.clone()
        }
    }

    pub(crate) fn script_engine(&self) -> &Engine {
        self.script_engine.get_or_init(script::engine)
    }

    /// The script compiled with `compile`, once per call for every source.
    pub(crate) fn script_ast<E>(
        &self,
        source: &str,
        compile: impl FnOnce(&Engine, &str) -> Result<AST, E>,
    ) -> Result<Rc<AST>, E> {
        if let Some(ast) = self.scripts.borrow().get(source) {
            return Ok(ast.clone());
        }
        let ast = Rc::new(compile(self.script_engine(), source)?);
        self.scripts.borrow_mut().insert(source.into(), ast.clone());
        Ok(ast)
    }

    /// The node the steps are evaluated for, entries created by a step belong to it.
    pub(crate) fn origin(&self) -> NodeHandle {
        self.origin
    }

    /// The mode the match is currently played in.
    pub(crate) fn match_mode(&self, node: NodeHandle) -> Option<TmMode> {
        let NodeHandle::MatchV1(match_id) = node else {
            return None;
        };
        Some(self.match_state_find(match_id)?.get_mode())
    }

    /// The mode settings of the config the match is currently played with.
    pub(crate) fn mode_settings(&self, node: NodeHandle) -> Option<ModeSettingsV2> {
        let NodeHandle::MatchV1(match_id) = node else {
            return None;
        };
        let tm_match = self.match_find(match_id)?;
        let config = self
            .raw_server_config(tm_match.get_active_config_id())
            .ok()?;
        Some(config.get_mode().clone())
    }
}

impl CtxDbRead for LbContext {
    fn db_read_only(&self) -> &LocalReadOnly {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{
        LbSettingsV3, evaluate_steps,
        steps::script::{LbScriptOutput, LbScriptSettings},
        test_util::{MATCH, round, test_context},
    };

    #[test]
    fn the_nodes_of_a_call_share_one_script_engine() {
        let test = test_context(&[]);
        let cx = LbContext::new(&test.anonymous_view_context(), MATCH);

        let other = cx.at(NodeHandle::LeaderboardV1(2));

        assert!(std::ptr::eq(cx.script_engine(), other.script_engine()));
        assert_eq!(other.origin(), NodeHandle::LeaderboardV1(2));
    }

    #[test]
    fn the_script_engine_is_only_built_when_a_script_runs() {
        let test = test_context(&[]);
        let cx = LbContext::new(&test.anonymous_view_context(), MATCH);
        let script = LbSettingsV3::Script(LbScriptSettings {
            source: "fn step(cx, entries) { entries }".into(),
            output: LbScriptOutput::Rounds,
        });

        evaluate_steps(&cx, &[LbSettingsV3::Match], vec![round(1, 1, 10, 0)]).ok();
        assert!(cx.script_engine.get().is_none());

        evaluate_steps(&cx, &[script], vec![round(1, 1, 10, 0)]).unwrap();
        assert!(cx.script_engine.get().is_some());
    }

    #[test]
    fn a_script_evaluated_for_several_nodes_of_a_call_compiles_once() {
        let test = test_context(&[]);
        let cx = LbContext::new(&test.anonymous_view_context(), MATCH);
        let other = cx.at(NodeHandle::LeaderboardV1(2));
        let source = "fn step(cx, entries) { entries }";
        let compiles = std::cell::Cell::new(0);
        let compile = |engine: &Engine, source: &str| {
            compiles.set(compiles.get() + 1);
            engine.compile(source)
        };

        let first = cx.script_ast(source, compile).unwrap();
        let again = other.script_ast(source, compile).unwrap();
        let changed = other
            .script_ast("fn step(cx, entries) { [] }", compile)
            .unwrap();

        assert_eq!(
            compiles.get(),
            2,
            "the same source compiles once, another one too"
        );
        assert!(Rc::ptr_eq(&first, &again));
        assert!(!Rc::ptr_eq(&first, &changed));
        // Nothing is kept across calls.
        let next = LbContext::new(&test.anonymous_view_context(), MATCH);
        next.script_ast(source, compile).unwrap();
        assert_eq!(compiles.get(), 3);
    }
}
