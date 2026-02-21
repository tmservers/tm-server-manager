use spacetimedb::{ReducerContext, ScheduleAt, Table, table};

use crate::project::{ProjectStatus, ProjectV1, tab_project};

#[table(accessor= tab_project_status_schedule, scheduled(on_project_status_schedule_triggered))]
pub struct ProjectStatusScheduleV1 {
    #[primary_key]
    #[auto_inc]
    pub scheduled_id: u64,

    pub scheduled_at: ScheduleAt,

    #[unique]
    pub tournament_id: u32,
    pub new_status: ProjectStatus,
}

impl ProjectStatusScheduleV1 {
    pub(crate) fn get_project(&self) -> u32 {
        self.tournament_id
    }

    pub(crate) fn get_new_status(&self) -> ProjectStatus {
        self.new_status
    }
}

#[cfg_attr(feature = "spacetime", spacetimedb::reducer)]
fn on_project_status_schedule_triggered(
    ctx: &ReducerContext,
    arg: ProjectStatusScheduleV1,
) -> Result<(), String> {
    if !ctx.sender_auth().is_internal() {
        return Err("Only the Databse is permitted to call this reducer.".into());
    }

    let Some(mut tournament) = ctx.db.tab_project().id().find(arg.get_project()) else {
        return Err("Invalid tournament".into());
    };

    tournament.status = arg.get_new_status();

    tournament = ctx.db.tab_project().id().update(tournament);

    // Schedule the next status change if applicable
    schedule_project_status_change(ctx, &tournament)?;

    Ok(())
}

// Internal function to automatically schedule the next status change for a tournament
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
        tournament_id: project.id,
        new_status,
    };

    // If there is an existing scheduled status change for this tournament, remove it
    ctx.db
        .tab_project_status_schedule()
        .tournament_id()
        .delete(schedule.tournament_id);

    ctx.db.tab_project_status_schedule().try_insert(schedule)?;

    Ok(())
}
