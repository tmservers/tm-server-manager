use spacetimedb::{ReducerContext, Uuid, reducer, table};

use crate::{authorization::Authorization, project::permissions::ProjectPermissionsV1};

#[table(accessor= tab_project_role)]
pub struct ProjectRole {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    #[index(hash)]
    project_id: u32,

    permissions: u64,
}

impl ProjectRole {
    pub(crate) fn get_permissions1(&self) -> ProjectPermissionsV1 {
        ProjectPermissionsV1(self.permissions)
    }
}

#[table(accessor= tab_project_role_members,index(accessor= user_roles , hash(columns= [role_id,account_id])))]
pub struct ProjectRoleMember {
    #[index(hash)]
    role_id: u32,

    #[index(hash)]
    account_id: u32,
}

impl ProjectRoleMember {
    pub fn get_role_id(&self) -> u32 {
        self.role_id
    }
}

#[table(accessor= tab_project_member,index(accessor= user_roles , hash(columns= [project_id,account_id])))]
pub struct ProjectMember {
    #[index(hash)]
    project_id: u32,

    #[index(hash)]
    account_id: u32,

    permissions: u64,
}

#[reducer]
pub fn project_member_add(
    ctx: &ReducerContext,
    project_id: u32,
    account_id: Uuid,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;
    ctx.auth_builder(project_id, account_id)?
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    //TODO

    Ok(())
}
