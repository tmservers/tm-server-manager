use spacetimedb::{
    Query, ReducerContext, ScheduleAt, Table, Timestamp, ViewContext, reducer, table, view,
};

use crate::competition::tab_competition;

#[table(name = tab_schedule, scheduled(on_schedule_triggered))]
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

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn on_schedule_triggered(ctx: &ReducerContext, arg: ScheduleV1) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }
    /* let message_to_send = arg.text;

    _ = send_message(ctx, message_to_send); */

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

#[view(name= schedule,public)]
pub fn schedule(ctx: &ViewContext) -> Query<ScheduleV1> {
    ctx.from.tab_schedule().build()
}
