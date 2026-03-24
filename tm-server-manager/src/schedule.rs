use spacetimedb::{
    Query, ReducerContext, ScheduleAt, SpacetimeType, Table, TimeDuration, Timestamp, ViewContext,
    reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1, connection::internal_graph_resolution_node_finished,
        node::NodeKindHandle, tab_competition,
    },
};

#[table(accessor= tab_schedule)]
pub struct ScheduleV1 {
    name: String,

    #[primary_key]
    #[auto_inc]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,

    settings: ScheduleSettings,

    status: ScheduleStatus,

    template: bool,
}

impl ScheduleV1 {
    pub(crate) fn parent_id(&self) -> u32 {
        self.parent_id
    }

    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn instantiate(mut self, parent_id: u32, stay_template: bool) -> Self {
        self.template = stay_template;
        self.parent_id = parent_id;
        self.id = 0;
        self
    }

    pub(crate) fn can_mutate_settings(&self) -> Result<(), String> {
        if !self.status.before_live() {
            return Err("Schedule is not before live.".into());
        }
        //TODO maybe there are more states?
        Ok(())
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub enum ScheduleSettings {
    Manual,
    Absolute(Timestamp),
    Relative(TimeDuration),
    //TODO when there are connections
    //Rounded(RoundedSettings),
}

impl ScheduleSettings {
    fn eval(&self, now: Timestamp) -> Timestamp {
        match self {
            ScheduleSettings::Manual => unreachable!(),
            ScheduleSettings::Absolute(timestamp) => *timestamp,
            ScheduleSettings::Relative(time_duration) => now + *time_duration,
            //ScheduleSettings::Rounded(rounded_settings) => todo!(),
        }
    }
}

/* #[derive(Debug, SpacetimeType)]
enum RoundedSettings {
    NextFullHour,
    Next15Minutes,
    Next10Minutes,
    Next5Minutes,
} */

#[derive(Debug, SpacetimeType, PartialEq, Eq, Clone, Copy)]
enum ScheduleStatus {
    Configuring,
    Waiting,
    Finished,
    Locked,
}

impl ScheduleStatus {
    fn before_live(&self) -> bool {
        match self {
            ScheduleStatus::Configuring => true,
            ScheduleStatus::Waiting => true,
            ScheduleStatus::Finished => false,
            ScheduleStatus::Locked => false,
        }
    }
}

#[table(accessor= tab_schedule_exec, scheduled(on_schedule_exec))]
pub struct ScheduleExecV1 {
    #[primary_key]
    pub scheduled_id: u64,

    scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer]
fn on_schedule_exec(ctx: &ReducerContext, arg: ScheduleExecV1) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }

    //TODO modify the Schedule state in here.

    internal_graph_resolution_node_finished(
        ctx,
        //arg.competition_id,
        NodeKindHandle::ScheduleV1(arg.scheduled_id as u32),
    )?;

    Ok(())
}

#[reducer]
pub fn schedule_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    with_template: u32,
) -> Result<(), String> {
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::SCHEDULE_CREATE)
        .authorize()?;

    if ctx
        .db
        .tab_competition()
        .id()
        .find(parent_id)
        .unwrap()
        .is_template()
    {
        return Err("Cannot add a normal node to a match".into());
    };

    if with_template != 0 {
        let Some(schedule) = ctx.db.tab_schedule().id().find(with_template) else {
            return Err("Template not found!".into());
        };
        //TODO do we have access to this template?
        let new_registration = schedule.instantiate(parent_id, false);
        ctx.db.tab_schedule().try_insert(new_registration)?;
    } else {
        let schedule = ScheduleV1 {
            id: 0,
            parent_id,
            template: false,
            settings: ScheduleSettings::Absolute(ctx.timestamp),
            status: ScheduleStatus::Configuring,
            name,
        };

        ctx.db.tab_schedule().try_insert(schedule)?;
    }
    Ok(())
}

#[reducer]
pub fn schedule_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut schedule) = ctx.db.tab_schedule().id().find(id) else {
        return Err("Invalid schedule".into());
    };

    ctx.auth_builder(schedule.parent_id)
        .permission(CompetitionPermissionsV1::SCHEDULE_CREATE)
        .authorize()?;

    if schedule.status != ScheduleStatus::Configuring {
        return Err("Schedule is already configured".into());
    }

    schedule.status = ScheduleStatus::Waiting;

    Ok(())
}

//TODO codegen bug
/* #[reducer]
pub fn schedule_settings(
    ctx: &ReducerContext,
    id: u32,
    settings: ScheduleSettings,
) -> Result<(), String> {
    let Some(mut schedule) = ctx.db.tab_schedule().id().find(id) else {
        return Err("Invalid schedule".into());
    };

    ctx.auth_builder(schedule.parent_id)
        .permission(CompetitionPermissionsV1::SCHEDULE_CREATE)
        .authorize()?;

    schedule.can_mutate_settings()?;

    schedule.settings = settings;

    Ok(())
}
 */
#[reducer]
pub fn schedule_try_run(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut schedule) = ctx.db.tab_schedule().id().find(id) else {
        return Err("Invalid schedule".into());
    };

    ctx.auth_builder(schedule.parent_id)
        .permission(CompetitionPermissionsV1::SCHEDULE_CREATE)
        .authorize()?;

    if schedule.status != ScheduleStatus::Waiting {
        return Err("Schedule cannot be started".into());
    }

    let timestamp = schedule.settings.eval(ctx.timestamp);

    //TODO maybe check if timestamp is in the past?

    //TODO manual schedule exec mode.

    schedule.status = ScheduleStatus::Waiting;

    ctx.db.tab_schedule().id().update(schedule);

    ctx.db.tab_schedule_exec().try_insert(ScheduleExecV1 {
        scheduled_id: id as u64,
        scheduled_at: ScheduleAt::Time(timestamp),
    })?;

    Ok(())
}

#[view(accessor=comeptition_schedules,public)]
pub fn comeptition_schedules(
    ctx: &ViewContext, /* , competition_id: u32 */
) -> impl Query<ScheduleV1> {
    let competition_id = 1u32;
    ctx.from
        .tab_schedule()
        .r#where(|f| f.parent_id.eq(competition_id))
}
