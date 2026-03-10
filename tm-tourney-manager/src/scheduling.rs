use spacetimedb::{
    Query, ReducerContext, ScheduleAt, Table, Timestamp, ViewContext, reducer, table, view,
};

use crate::competition::{
    connection::{NodeKindHandle, internal_graph_resolution_node_finished},
    tab_competition,
};

#[table(accessor= tab_schedule, scheduled(on_schedule_triggered))]
pub struct ScheduleV1 {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,

    scheduled_at: ScheduleAt,

    #[index(hash)]
    parent_id: u32,
    project_id: u32,

    template: bool,
}

impl ScheduleV1 {
    pub(crate) fn parent_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn get_project(&self) -> u32 {
        self.project_id
    }

    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn instantiate(mut self, parent_id: u32) -> Self {
        self.template = false;
        self.parent_id = parent_id;
        self.scheduled_id = 0;
        self
    }
}

#[spacetimedb::reducer]
pub(crate) fn on_schedule_triggered(ctx: &ReducerContext, arg: ScheduleV1) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }

    internal_graph_resolution_node_finished(
        ctx,
        //arg.competition_id,
        NodeKindHandle::SchedulingV1(arg.scheduled_id as u32),
    )?;

    Ok(())
}

#[reducer]
pub fn create_schedule(
    ctx: &ReducerContext,
    parent_id: u32,
    scheduled_at: Timestamp,
) -> Result<(), String> {
    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    let schedule = ScheduleV1 {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(scheduled_at),
        parent_id,
        project_id: parent_competition.get_project(),
        template: false,
    };

    ctx.db.tab_schedule().try_insert(schedule)?;

    Ok(())
}

#[view(accessor= schedule,public)]
pub fn schedule(ctx: &ViewContext) -> impl Query<ScheduleV1> {
    ctx.from.tab_schedule().build()
}
