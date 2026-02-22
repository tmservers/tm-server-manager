use spacetimedb::{ReducerContext, ScheduleAt, Table, table};

use crate::project::{ProjectStatus, ProjectV1, tab_project};

#[table(accessor= tab_project_status_schedule, scheduled(on_project_status_schedule_triggered))]
pub struct ProjectStatusScheduleV1 {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,

    pub scheduled_at: ScheduleAt,

    #[unique]
    pub project_id: u32,
    pub new_status: ProjectStatus,
}

impl ProjectStatusScheduleV1 {
    pub(crate) fn get_project(&self) -> u32 {
        self.project_id
    }

    pub(crate) fn get_new_status(&self) -> ProjectStatus {
        self.new_status
    }
}

#[spacetimedb::reducer]
fn on_project_status_schedule_triggered(
    ctx: &ReducerContext,
    arg: ProjectStatusScheduleV1,
) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }

    let Some(mut project) = ctx.db.tab_project().id().find(arg.get_project()) else {
        return Err("Invalid project".into());
    };

    project.status = arg.get_new_status();

    project = ctx.db.tab_project().id().update(project);

    // Schedule the next status change if applicable
    schedule_project_status_change(ctx, &project)?;

    Ok(())
}

// Internal function to automatically schedule the next status change for a project
pub fn schedule_project_status_change(
    ctx: &ReducerContext,
    project: &ProjectV1,
) -> Result<(), String> {
    let (new_status, scheduled_at) = match project.status {
        ProjectStatus::Announced => (ProjectStatus::Ongoing, project.starting_at),
        ProjectStatus::Ongoing => (ProjectStatus::Ended, project.ending_at),
        _ => {
            // No scheduling needed for Planning or Ended status
            return Ok(());
        }
    };

    let schedule = ProjectStatusScheduleV1 {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Time(scheduled_at),
        project_id: project.id,
        new_status,
    };

    // If there is an existing scheduled status change for this project, remove it
    ctx.db
        .tab_project_status_schedule()
        .project_id()
        .delete(schedule.project_id);

    ctx.db.tab_project_status_schedule().try_insert(schedule)?;

    Ok(())
}
