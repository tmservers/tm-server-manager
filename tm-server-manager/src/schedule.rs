use spacetimedb::{
    CtxDbRead, ReducerContext, ScheduleAt, SpacetimeType, Table, TimeDuration, Timestamp, reducer,
    table,
};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1, CompetitionRead,
        connection::internal_graph_resolution_node_finished,
        node::{NodeHandle, NodeWrite, Vec2},
    },
};

#[table(accessor= tab_schedule,vis_private)]
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

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn is_template(&self) -> bool {
        self.template
    }

    pub(crate) fn can_mutate_settings(&self) -> Result<(), String> {
        if !self.status.before_finished() {
            return Err("Schedule is not before live.".into());
        }
        Ok(())
    }
}

#[derive(Debug, SpacetimeType, Clone, Copy)]
pub enum ScheduleSettings {
    Manual,
    Absolute(Timestamp),
    Relative(TimeDuration),
}

#[derive(Debug, SpacetimeType, PartialEq, Eq, Clone, Copy)]
enum ScheduleStatus {
    Configuring,
    Configured,
    Waiting,
    Finished,
    Locked,
}

impl ScheduleStatus {
    fn before_finished(&self) -> bool {
        match self {
            ScheduleStatus::Configuring => true,
            ScheduleStatus::Configured => true,
            ScheduleStatus::Waiting => true,
            ScheduleStatus::Finished => false,
            ScheduleStatus::Locked => false,
        }
    }
}

#[table(accessor= tab_schedule_exec, scheduled(on_schedule_exec))]
struct ScheduleExecV1 {
    #[auto_inc]
    #[primary_key]
    scheduled_id: u64,

    schedule_id: u32,
    scheduled_at: ScheduleAt,
}

#[spacetimedb::reducer]
fn on_schedule_exec(ctx: &ReducerContext, arg: ScheduleExecV1) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }

    let Some(mut schedule) = ctx.db.tab_schedule().id().find(arg.schedule_id) else {
        return Err("Invalid schedule".into());
    };

    schedule.status = ScheduleStatus::Finished;

    ctx.db.tab_schedule().id().update(schedule);

    internal_graph_resolution_node_finished(ctx, NodeHandle::ScheduleV1(arg.schedule_id))?;

    Ok(())
}

#[reducer]
fn schedule_create(
    ctx: &ReducerContext,
    name: String,
    parent_id: u32,
    position: Vec2,
    with_template: u32,
    as_template: bool,
) -> Result<(), String> {
    ctx.auth_builder(parent_id)
        .permission(CompetitionPermissionsV1::SCHEDULE_CREATE)
        .authorize()?;

    if ctx.competition_find(parent_id)?.is_template() {
        return Err("Cannot add a normal node to a match".into());
    };

    if with_template != 0 {
        todo!()
    } else {
        ScheduleWrite::schedule_create(ctx, name, parent_id, position, None, as_template)?;
    }

    Ok(())
}

#[reducer]
fn schedule_configured(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut schedule) = ctx.db.tab_schedule().id().find(id) else {
        return Err("Invalid schedule".into());
    };

    ctx.auth_builder(schedule.parent_id)
        .permission(CompetitionPermissionsV1::SCHEDULE_CREATE)
        .authorize()?;

    if schedule.status != ScheduleStatus::Configuring {
        return Err("Schedule is already configured".into());
    }

    if matches!(schedule.settings, ScheduleSettings::Absolute(_)) {
        if schedule.is_template() {
            return Err(
                "Cannot set a absolute schedule to configured in a template. Please do it when instantiated.".into(),
            );
        }
        schedule.status = ScheduleStatus::Waiting;
        ctx.db.tab_schedule_exec().try_insert(ScheduleExecV1 {
            scheduled_id: 0,
            schedule_id: id,
            scheduled_at: match schedule.settings {
                ScheduleSettings::Absolute(time) => ScheduleAt::Time(time),
                _ => unreachable!(),
            },
        })?;
    } else {
        schedule.status = ScheduleStatus::Configured;
    }

    ctx.db.tab_schedule().id().update(schedule);

    Ok(())
}

#[reducer]
fn schedule_settings_update(
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

    ctx.db.tab_schedule().id().update(schedule);

    Ok(())
}

#[reducer]
fn schedule_manual_run(ctx: &ReducerContext, id: u32) -> Result<(), String> {
    let Some(mut schedule) = ctx.db.tab_schedule().id().find(id) else {
        return Err("Invalid schedule".into());
    };

    if schedule.is_template() {
        return Err("Cannot manually run a template schedule".into());
    }

    ctx.auth_builder(schedule.parent_id)
        //TODO
        //.permission(CompetitionPermissionsV1::SCHEDULE_CREATE)
        .authorize()?;

    if schedule.status != ScheduleStatus::Configured {
        return Err("Schedule cannot be started".into());
    }

    //Maybe we should also allow to run non manual schedules?
    if !matches!(schedule.settings, ScheduleSettings::Manual) {
        return Err("Tried to manually run a non manual schedule.".into());
    }

    schedule.status = ScheduleStatus::Finished;

    ctx.db.tab_schedule().id().update(schedule);

    internal_graph_resolution_node_finished(ctx, NodeHandle::ScheduleV1(id))?;

    Ok(())
}

/* #[view(accessor=my_comeptition_schedules,public)]
pub fn my_comeptition_schedules(
    ctx: &ViewContext, /* , competition_id: u32 */
) -> impl Query<ScheduleV1> {
    let competition_id = 1u32;
    ctx.from
        .tab_schedule()
        .r#where(|f| f.parent_id.eq(competition_id))
} */

pub(crate) trait ScheduleRead {
    fn schedule_find(&self, id: u32) -> Result<ScheduleV1, String>;
    fn schedules_with_competition(&self, competition_id: u32) -> impl Iterator<Item = ScheduleV1>;
}

impl<Db: CtxDbRead> ScheduleRead for Db {
    fn schedule_find(&self, id: u32) -> Result<ScheduleV1, String> {
        let Some(schedule) = self.db_read_only().tab_schedule().id().find(id) else {
            return Err("Leaderboard not found!".into());
        };
        Ok(schedule)
    }

    fn schedules_with_competition(&self, competition_id: u32) -> impl Iterator<Item = ScheduleV1> {
        self.db_read_only()
            .tab_schedule()
            .parent_id()
            .filter(competition_id)
    }
}

pub(crate) trait ScheduleWrite {
    fn schedule_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<ScheduleV1>,
        as_template: bool,
    ) -> Result<ScheduleV1, String>;
    fn schedule_start_relative(&self, schedule_id: u32, now: Timestamp) -> Result<(), String>;
    fn schedule_name_edit(&self, match_id: u32, name: String) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> ScheduleWrite for Db {
    fn schedule_start_relative(&self, schedule_id: u32, now: Timestamp) -> Result<(), String> {
        let Some(mut schedule) = self.db().tab_schedule().id().find(schedule_id) else {
            return Err("Invalid schedule".into());
        };
        schedule.status = ScheduleStatus::Waiting;
        self.db().tab_schedule_exec().try_insert(ScheduleExecV1 {
            scheduled_id: 0,
            schedule_id,
            scheduled_at: match schedule.settings {
                ScheduleSettings::Relative(time) => ScheduleAt::Time(now + time),
                _ => unreachable!(),
            },
        })?;

        self.db().tab_schedule().id().update(schedule);
        Ok(())
    }

    fn schedule_name_edit(&self, match_id: u32, name: String) -> Result<(), String> {
        let Some(mut tm_match) = self.db().tab_schedule().id().find(match_id) else {
            return Err("Match not found.".into());
        };
        tm_match.name = name;
        self.db().tab_schedule().id().update(tm_match);

        Ok(())
    }

    fn schedule_create(
        &self,
        name: String,
        parent_id: u32,
        position: Vec2,
        template: Option<ScheduleV1>,
        as_template: bool,
    ) -> Result<ScheduleV1, String> {
        let schedule = if let Some(mut template) = template {
            template.template = as_template; //TODO: Audit codepath for legal again.
            template.parent_id = parent_id;
            template.id = 0;
            template
        } else {
            ScheduleV1 {
                id: 0,
                parent_id,
                template: false,
                settings: ScheduleSettings::Manual,
                status: ScheduleStatus::Configuring,
                name,
            }
        };

        let tm_match = self.db().tab_schedule().try_insert(schedule)?;
        self.node_create(NodeHandle::ScheduleV1(tm_match.id), position)?;

        Ok(tm_match)
    }
}
