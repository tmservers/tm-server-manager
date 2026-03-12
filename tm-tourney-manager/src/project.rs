use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, Timestamp, Uuid,
    ViewContext, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{CompetitionV1, tab_competition},
    user::{tab_user__view, tab_user_identity__view},
};

pub(crate) mod permissions;
pub mod roles;
pub mod servers;
mod status_schedule;

/// A project is a logical grouping of competitions and also the only way to obtain a competition in the first place.
/// It does not provide functionality in of itself but is responsible for all the metadata.
#[table(accessor= tab_project)]
pub struct ProjectV1 {
    #[unique]
    name: String,
    description: String,

    #[index(btree)]
    creator_account_id: Uuid,

    starting_at: Timestamp,
    ending_at: Timestamp,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    status: ProjectStatus,
}

impl ProjectV1 {}

#[derive(Debug, PartialEq, Eq, Clone, Copy, SpacetimeType)]
pub enum ProjectStatus {
    // public API cant query it
    Planning,
    // API is public
    Announced,
    // Competitions have started
    Ongoing,
    // Whole Project finshed
    Ended,
}

impl ProjectStatus {
    //TODO this method cannot be used because of custom type
    fn is_public(&self) -> bool {
        *self != ProjectStatus::Planning
    }
}

/// Requires name, description, starting and ending timestamps.
/// Description can be empty.
#[reducer]
fn create_project(
    ctx: &ReducerContext,
    name: String,
    description: String,
    starting_at: Timestamp,
    ending_at: Timestamp,
) -> Result<(), String> {
    let user = ctx.get_user()?;

    let project = ctx.db.tab_project().try_insert(ProjectV1 {
        id: 0,
        name: name.clone(),
        creator_account_id: user.account_id,
        status: ProjectStatus::Planning,
        description,
        starting_at,
        ending_at,
    })?;

    //SAFETY: Comitted afterwards
    let competition = unsafe { CompetitionV1::new(name, 0) };
    ctx.db.tab_competition().try_insert(competition)?;

    Ok(())
}

#[spacetimedb::reducer]
fn project_edit_name(ctx: &ReducerContext, project_id: u32, name: String) -> Result<(), String> {
    let user = ctx.get_user()?;
    /* ctx.auth_builder(project_id, user.account_id)?
    .permission(CompetitionPermissionsV1::PROJECT_EDIT_NAME)
    .authorize()?; */

    let Some(mut project) = ctx.db.tab_project().id().find(project_id) else {
        return Err("Supplied project_id incorrect.".into());
    };

    project.name = name;

    ctx.db.tab_project().id().update(project);

    Ok(())
}

#[spacetimedb::reducer]
fn project_edit_description(
    ctx: &ReducerContext,
    project_id: u32,
    description: String,
) -> Result<(), String> {
    let user = ctx.get_user()?;
    /* ctx.auth_builder(project_id, user.account_id)?
    .permission(CompetitionPermissionsV1::PROJECT_EDIT_DESCRIPTION)
    .authorize()?; */

    let Some(mut project) = ctx.db.tab_project().id().find(project_id) else {
        return Err("Supplied project_id incorrect.".into());
    };

    project.description = description;

    ctx.db.tab_project().id().update(project);

    Ok(())
}

#[spacetimedb::reducer]
fn project_edit_dates(
    ctx: &ReducerContext,
    project_id: u32,
    starting_at: Timestamp,
    ending_at: Timestamp,
) -> Result<(), String> {
    let user = ctx.get_user()?;
    /* ctx.auth_builder(project_id, user.account_id)?
    .permission(CompetitionPermissionsV1::PROJECT_EDIT_DATE)
    .authorize()?; */

    let Some(mut project) = ctx.db.tab_project().id().find(project_id) else {
        return Err("Supplied project_id incorrect.".into());
    };

    let current_time = ctx.timestamp;

    if project.status != ProjectStatus::Planning {
        // Don't allow modifying starting_at if project already started
        if project.starting_at != starting_at && current_time >= project.starting_at {
            return Err("Cannot modify start date of a project that has already started.".into());
        }

        // Don't allow modifying ending_at if project already ended
        if project.ending_at != ending_at && current_time >= project.ending_at {
            return Err("Cannot modify end date of a project that has already ended.".into());
        }
    }

    // Don't allow modifying ending_at to before starting_at
    if ending_at < starting_at {
        return Err("Ending date cannot be before starting date.".into());
    }

    project.starting_at = starting_at;
    project.ending_at = ending_at;

    // Check if the current status needs to be updated based on the new dates
    if project.status == ProjectStatus::Announced && current_time >= starting_at {
        // Announced and starting time passed
        project.status = ProjectStatus::Ongoing;

        if current_time >= ending_at {
            // Ending time also passed
            project.status = ProjectStatus::Ended;
        }
    } else if project.status == ProjectStatus::Ongoing && current_time >= ending_at {
        // Ongoing and ending time passed
        project.status = ProjectStatus::Ended;
    }

    project = ctx.db.tab_project().id().update(project);

    // Schedule the next status change if applicable
    //  status_schedule::schedule_project_status_change(ctx, &project)?;

    Ok(())
}

#[spacetimedb::reducer]
fn project_update_status(ctx: &ReducerContext, project_id: u32) -> Result<(), String> {
    let user = ctx.get_user()?;

    let Some(mut project) = ctx.db.tab_project().id().find(project_id) else {
        return Err("Supplied project_id incorrect.".into());
    };

    if project.status != ProjectStatus::Planning {
        return Err("Project status can only be updated from Planning state.".into());
    }

    let current_time = ctx.timestamp;

    if current_time < project.starting_at {
        project.status = ProjectStatus::Announced;
    } else if current_time >= project.starting_at && current_time < project.ending_at {
        project.status = ProjectStatus::Ongoing;
    } else {
        project.status = ProjectStatus::Ended;
    }

    project = ctx.db.tab_project().id().update(project);

    // Schedule the next status change
    //status_schedule::schedule_project_status_change(ctx, &project)?;

    Ok(())
}

#[view(accessor=project,public)]
pub fn project(ctx: &AnonymousViewContext) -> impl Query<ProjectV1> {
    ctx.from
        .tab_project()
        //TODO this equality doesnt work atm because of enum
        //.r#where(|t| t.status.ne(ProjectStatus::Planning))
        .build()
}

#[derive(Debug, SpacetimeType)]
pub struct MyProjectV1 {
    id: u32,

    creator_account_id: Uuid,
    creator_name: String,

    name: String,

    starting_at: Timestamp,
    ending_at: Timestamp,

    description: String,

    status: ProjectStatus,
}

#[view(accessor=my_project,public)]
pub fn my_project(ctx: &ViewContext) -> Vec<MyProjectV1> {
    let id = if let Some(user) = ctx.db.tab_user_identity().identity().find(ctx.sender()) {
        user.account_id
    } else {
        return Vec::new();
    };

    let Some(user) = ctx.db.tab_user().account_id().find(id) else {
        return Vec::new();
    };

    ctx.db
        .tab_project()
        .creator_account_id()
        .filter(id)
        .map(|t| MyProjectV1 {
            id: t.id,
            creator_account_id: t.creator_account_id,
            creator_name: user.get_name().to_string(),
            name: t.name,
            starting_at: t.starting_at,
            ending_at: t.ending_at,
            description: t.description,
            status: t.status,
        })
        .collect()
}

#[table(accessor=tab_test_tournament)]
struct TestTournament {
    name: String,

    #[auto_inc]
    #[primary_key]
    id: u32,

    root_id: u32,
}
