use spacetimedb::{
    Query, ReducerContext, ScheduleAt, SpacetimeType, Table, TimeDuration, Timestamp, ViewContext,
    reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{
        connection::{NodeKindHandle, internal_graph_resolution_node_finished},
        tab_competition,
    },
    project::permissions::ProjectPermissionsV1,
};

#[table(accessor= tab_schedule, /* scheduled(on_schedule_triggered) */)]
pub struct ScheduleV1 {
    name: String,

    #[primary_key]
    #[auto_inc]
    pub id: u32,

    #[index(hash)]
    parent_id: u32,
    project_id: u32,

    settings: ScheduleSettings,

    state: ScheduleState,

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

    pub(crate) fn instantiate(mut self, parent_id: u32, stay_template: bool) -> Self {
        self.template = stay_template;
        self.parent_id = parent_id;
        self.id = 0;
        self
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
enum ScheduleSettings {
    Absolute(Timestamp),
    Relative(TimeDuration),
    //TODO when there are connections
    //Rounded(RoundedSettings),
}

impl ScheduleSettings {
    fn eval(&self, now: Timestamp) -> Timestamp {
        match self {
            ScheduleSettings::Absolute(timestamp) => *timestamp,
            ScheduleSettings::Relative(time_duration) => now + *time_duration,
            //ScheduleSettings::Rounded(rounded_settings) => todo!(),
        }
    }
}

#[derive(Debug, SpacetimeType)]
enum RoundedSettings {
    NextFullHour,
    Next15Minutes,
    Next10Minutes,
    Next5Minutes,
}

#[derive(Debug, SpacetimeType, PartialEq, Eq, Clone, Copy)]
enum ScheduleState {
    Configuring,
    Waiting,
    Ended,
    Locked,
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
    //settings: ScheduleSettings,
) -> Result<(), String> {
    let user = ctx.get_user_account()?;

    let Some(parent_competition) = ctx.db.tab_competition().id().find(parent_id) else {
        return Err("Invalid competition".into());
    };

    //TODO permission fix
    ctx.auth_builder(parent_competition.get_project(), user)?
        .permission(ProjectPermissionsV1::MATCH_CREATE)
        .authorize()?;

    let schedule = ScheduleV1 {
        id: 0,
        parent_id,
        project_id: parent_competition.get_project(),
        template: false,
        settings: ScheduleSettings::Absolute(ctx.timestamp),
        state: ScheduleState::Configuring,
        name,
    };

    ctx.db.tab_schedule().try_insert(schedule)?;

    Ok(())
}

#[reducer]
pub fn schedule_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let user = ctx.get_user_account()?;

    let Some(schedule) = ctx.db.tab_schedule().id().find(id) else {
        return Err("Invalid schedule".into());
    };

    //TODO permission fix
    ctx.auth_builder(schedule.get_project(), user)?
        .permission(ProjectPermissionsV1::MATCH_CREATE)
        .authorize()?;

    if schedule.state == ScheduleState::Waiting {
        return Err("Schedule is already configured".into());
    }

    let timestamp = schedule.settings.eval(ctx.timestamp);

    ctx.db.tab_schedule_exec().try_insert(ScheduleExecV1 {
        scheduled_id: id as u64,
        scheduled_at: ScheduleAt::Time(timestamp),
    })?;

    Ok(())
}

#[view(accessor= schedule,public)]
pub fn schedule(ctx: &ViewContext) -> impl Query<ScheduleV1> {
    ctx.from.tab_schedule()
}
