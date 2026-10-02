use spacetimedb::{Local, ProcedureContext, ReducerContext, Table, procedure, reducer, table};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        node::{NodeHandle, NodeWrite, Vec2},
        tab_competition,
    },
};

#[table(accessor= tab_input)]
pub struct InputV1 {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    template: bool,
}

impl InputV1 {
    pub(crate) fn instantiate(mut self, parent_id: u32, stay_template: bool) -> Self {
        self.template = stay_template;
        self.parent_id = parent_id;
        self.id = 0;
        self
    }

    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn get_comp_id(&self) -> u32 {
        self.parent_id
    }
}

#[reducer]
fn input_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    position: Vec2,
    with_template: u32,
    as_template: bool,
) -> Result<(), String> {
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::INPUT_CREATE)
        .authorize()?;

    if parent_competition.is_template() {
        return Err(
            "Cannot add a normal server to a template. Try do add a template server to id.".into(),
        );
    }

    //TODO validation.

    // Try to load template if provided
    if with_template != 0 {
        ctx.input_template_instantiate(with_template)?;
    } else {
        // Create an uncommitted server
        let input = InputV1 {
            name,
            id: 0,
            parent_id,
            template: false,
        };

        let input = ctx.db.tab_input().try_insert(input)?;

        ctx.node_create(NodeHandle::InputV1(input.id), position)?;
    }

    Ok(())
}

pub(crate) trait InputRead {
    fn inputs_in_parent(&self, parent_id: u32) -> impl Iterator<Item = InputV1>;
}
impl<Db: spacetimedb::CtxDbRead> InputRead for Db {
    fn inputs_in_parent(&self, parent_id: u32) -> impl Iterator<Item = InputV1> {
        self.db_read_only()
            .tab_input()
            .parent_id()
            .filter(parent_id)
    }
}
pub(crate) trait InputWrite: InputRead {
    fn input_template_instantiate(&self, with_template: u32) -> Result<(), String>;
    fn input_insert(&self, input: InputV1) -> Result<InputV1, String>;
    fn input_name_edit(&self, input_id: u32, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> InputWrite for Db {
    fn input_template_instantiate(&self, with_template: u32) -> Result<(), String> {
        todo!()
    }

    fn input_insert(&self, input: InputV1) -> Result<InputV1, String> {
        self.db()
            .tab_input()
            .try_insert(input)
            .map_err(|e| e.to_string())
    }

    fn input_name_edit(&self, input_id: u32, name: String) -> Result<(), String> {
        let Some(mut tm_match) = self.db().tab_input().id().find(input_id) else {
            return Err("Match not found.".into());
        };
        tm_match.name = name;
        self.db().tab_input().id().update(tm_match);

        Ok(())
    }
}

#[procedure(name = "unstable_competition_inputs")]
fn unstable_competition_inputs(ctx: &mut ProcedureContext, competition_id: u32) -> Vec<InputV1> {
    ctx.with_tx(|ctx| ctx.db.tab_input().parent_id().filter(competition_id))
        .collect()
}
