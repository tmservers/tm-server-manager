use spacetimedb::{Local, ProcedureContext, ReducerContext, Table, procedure, reducer, table};

use crate::{
    authorization::Authorization, competition::{
        CompetitionPermissionsV1, node::{NodeHandle, NodeWrite, Vec2}, tab_competition,
    },
};

#[table(accessor= tab_output)]
pub struct OutputV1 {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    template: bool,
}

impl OutputV1 {
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
fn output_create(
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
        .permission(CompetitionPermissionsV1::OUTPUT_CREATE)
        .authorize()?;

    if parent_competition.is_template() {
        return Err(
            "Cannot add a normal output to a template. Try do add a template output to id.".into(),
        );
    }

    if ctx.db.tab_output().parent_id().filter(parent_id).count() != 0 {
        return Err("Only one output can be present per competition!".into());
    }

    // Try to load template if provided
    if with_template != 0 {
        ctx.output_template_instantiate(with_template)?;
    } else {
        let output = OutputV1 {
            name,
            id: 0,
            parent_id,
            template: false,
        };

        let output = ctx.db.tab_output().try_insert(output)?;

        ctx.node_create(NodeHandle::OutputV1(output.id), position)?;
    }

    Ok(())
}

pub(crate) trait OutputRead {
    fn outputs_in_parent(&self, parent_id: u32) -> impl Iterator<Item = OutputV1>;
}
impl<Db: spacetimedb::CtxDbRead> OutputRead for Db {
    fn outputs_in_parent(&self, parent_id: u32) -> impl Iterator<Item = OutputV1> {
        self.db_read_only()
            .tab_output()
            .parent_id()
            .filter(parent_id)
    }
}
pub(crate) trait OutputWrite: OutputRead {
    fn output_template_instantiate(&self, with_template: u32) -> Result<(), String>;
    fn output_insert(&self, output: OutputV1) -> Result<OutputV1, String>;
    fn output_name_edit(&self, output_id: u32, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> OutputWrite for Db {
    fn output_template_instantiate(&self, with_template: u32) -> Result<(), String> {
        todo!()
    }

    fn output_insert(&self, output: OutputV1) -> Result<OutputV1, String> {
        self.db()
            .tab_output()
            .try_insert(output)
            .map_err(|e| e.to_string())
    }

    fn output_name_edit(&self, output_id: u32, name: String) -> Result<(), String> {
        let Some(mut tm_match) = self.db().tab_output().id().find(output_id) else {
            return Err("Match not found.".into());
        };
        tm_match.name = name;
        self.db().tab_output().id().update(tm_match);

        Ok(())
    }
}

#[procedure(name = "unstable_competition_output")]
fn unstable_competition_output(ctx: &mut ProcedureContext, competition_id: u32) -> Vec<OutputV1> {
    ctx.with_tx(|ctx| ctx.db.tab_output().parent_id().filter(competition_id))
        .collect()
}
