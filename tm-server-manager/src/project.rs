use spacetimedb::{
    AnonymousViewContext, Query, ReducerContext, SpacetimeType, Table, Timestamp, Uuid,
    ViewContext, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{CompetitionRead, CompetitionV1, CompetitionWrite},
};

/// A project is a logical grouping of competitions and also the only way to obtain a competition in the first place.
/// It does not provide functionality in of itself but is responsible for all the metadata.
#[table(accessor= tab_project)]
pub struct ProjectV1 {
    #[unique]
    name: String,
    description: String,

    starting_at: Timestamp,
    ending_at: Timestamp,

    #[index(hash)]
    user_id: u32,

    #[auto_inc]
    #[primary_key]
    pub id: u32,

    //Verified by a instance admin and required to query it in the public api.
    verified: bool,
    kind: ProjectKind,
    status: ProjectStatus,

    root_competition: u32,
}

impl ProjectV1 {}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy, SpacetimeType)]
#[repr(u8)]
pub enum ProjectKind {
    Tournament,
    TournamentTest,
    GeneralProject,
}

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
    kind: ProjectKind,
    starting_at: Timestamp,
    ending_at: Timestamp,
) -> Result<(), String> {
    let user_id = ctx.user_id()?;

    let root_id = ctx.competition_root_create(user_id, name.clone())?;

    ctx.db.tab_project().try_insert(ProjectV1 {
        id: 0,
        name,
        user_id,
        status: ProjectStatus::Planning,
        description,
        starting_at,
        ending_at,
        verified: false,
        kind,
        root_competition: root_id,
    })?;

    Ok(())
}

#[spacetimedb::reducer]
fn project_edit_name(ctx: &ReducerContext, project_id: u32, name: String) -> Result<(), String> {
    //TODO auth

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
    //TODO auth
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
    let user_id = ctx.user_id()?;
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
    ctx.user_id()?;

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

/* #[view(accessor=project,public)]
pub fn project(ctx: &AnonymousViewContext) -> impl Query<ProjectV1> {
    ctx.from
        .tab_project()
        //TODO this equality doesnt work atm because of enum
        //.r#where(|t| t.status.ne(ProjectStatus::Planning))
        .build()
} */

/* #[derive(Debug, SpacetimeType)]
pub struct MyProjectV1 {
    id: u32,

    user_id: u32,
    creator_name: String,

    name: String,

    starting_at: Timestamp,
    ending_at: Timestamp,

    description: String,

    status: ProjectStatus,
    kind: ProjectKind,
    verified: bool,
} */

//TODO: walk the competition tree and check for permissions.
#[view(accessor=my_projects,public)]
pub fn my_projects(ctx: &ViewContext) -> Vec<ProjectV1> {
    let Ok(user_id) = ctx.user_id() else {
        return Vec::new();
    };

    ctx.db.tab_project().user_id().filter(user_id).collect()
}

#[view(accessor=project_competition_descendants,public)]
fn view_project_competition_descendants(
    ctx: &ViewContext, /* TODO project_id: u32 */
) -> Vec<CompetitionV1> {
    let project_id = 1u32;

    ctx.project_competition_descendants(project_id)
}

trait ProjectRead {
    fn project_competition_descendants(&self, project_id: u32) -> Vec<CompetitionV1>;
}
impl<Db: spacetimedb::CtxDbRead> ProjectRead for Db {
    fn project_competition_descendants(&self, project_id: u32) -> Vec<CompetitionV1> {
        let Some(project) = self.db_read_only().tab_project().id().find(project_id) else {
            return Vec::new();
        };

        self.competition_descendants(project.root_competition)
    }
}
