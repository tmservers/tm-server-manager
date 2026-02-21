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

    competition_id: u32,
    tournament_id: u32,
}

impl ScheduleV1 {
    pub(crate) fn get_comp_id(&self) -> u32 {
        self.competition_id
    }

    pub(crate) fn get_tournament(&self) -> u32 {
        self.tournament_id
    }
}

#[spacetimedb::reducer]
pub(crate) fn on_schedule_triggered(ctx: &ReducerContext, arg: ScheduleV1) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }

    internal_graph_resolution_node_finished(
        ctx,
        arg.competition_id,
        NodeKindHandle::SchedulingV1(arg.scheduled_id as u32),
    )?;

    Ok(())
}

#[reducer]
pub fn create_schedule(
    ctx: &ReducerContext,
    competition_id: u32,
    scheduled_at: Timestamp,
) -> Result<(), String> {
    let Some(parent_competition) = ctx.db.tab_competition().id().find(competition_id) else {
        return Err("Invalid competition".into());
    };

    let schedule = ScheduleV1 {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(scheduled_at),
        competition_id,
        tournament_id: parent_competition.get_tournament(),
    };

    ctx.db.tab_schedule().try_insert(schedule)?;

    Ok(())
}

#[view(accessor= schedule,public)]
pub fn schedule(ctx: &ViewContext) -> impl Query<ScheduleV1> {
    ctx.from.tab_schedule().build()
}
