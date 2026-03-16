/* use spacetimedb::{ReducerContext, Table, Uuid, reducer, table};

use crate::{
    authorization::Authorization, project::permissions::CompetitionPermissionsV1, user::tab_user,
};

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
    pub(crate) fn get_permissions1(&self) -> CompetitionPermissionsV1 {
        CompetitionPermissionsV1(self.permissions)
    }
}

#[table(accessor= tab_project_role_member,index(accessor= user_roles , hash(columns= [role_id,account_id])))]
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
    permissions: u64,

    #[auto_inc]
    #[primary_key]
    id: u32,

    #[index(hash)]
    project_id: u32,

    #[index(hash)]
    account_id: u32,
}

#[reducer]
pub fn project_member_add(
    ctx: &ReducerContext,
    project_id: u32,
    account_id: Uuid,
) -> Result<(), String> {
    let request_account_id = ctx.get_user_account()?;
    ctx.auth_builder(project_id, request_account_id)?
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    let Some(user) = ctx.db.tab_user().account_id().find(account_id) else {
        return Err("Account not found. If this is unexpected make sure the player has logged into the network once.".into());
    };

    ctx.db.tab_project_member().try_insert(ProjectMember {
        project_id,
        account_id: user.internal_id,
        permissions: 0,
        id: 0,
    })?;

    Ok(())
}

#[reducer]
pub fn project_member_remove(ctx: &ReducerContext, member_id: u32) -> Result<(), String> {
    let Some(project_member) = ctx.db.tab_project_member().id().find(member_id) else {
        return Err("Member with id not found!".into());
    };

    let account_id = ctx.get_user_account()?;
    ctx.auth_builder(project_member.project_id, account_id)?
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db.tab_project_member().id().delete(project_member.id);

    Ok(())
}

#[reducer]
pub fn project_role_create(
    ctx: &ReducerContext,
    project_id: u32,
    name: String,
) -> Result<(), String> {
    let account_id = ctx.get_user_account()?;
    ctx.auth_builder(project_id, account_id)?
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db.tab_project_role().try_insert(ProjectRole {
        name,
        id: 0,
        project_id,
        permissions: 0,
    })?;

    Ok(())
}

#[reducer]
pub fn project_role_remove(ctx: &ReducerContext, role_id: u32) -> Result<(), String> {
    let Some(role) = ctx.db.tab_project_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    let account_id = ctx.get_user_account()?;
    ctx.auth_builder(role.project_id, account_id)?
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db.tab_project_role().id().delete(role_id);

    for user in ctx.db.tab_project_role_member().role_id().filter(role_id) {
        ctx.db.tab_project_role_member().delete(user);
    }

    Ok(())
}

#[reducer]
pub fn project_role_member_assign(
    ctx: &ReducerContext,
    role_id: u32,
    account_id: Uuid,
) -> Result<(), String> {
    let Some(role) = ctx.db.tab_project_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    let request_account_id = ctx.get_user_account()?;
    ctx.auth_builder(role.project_id, request_account_id)?
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    let Some(user) = ctx.db.tab_user().account_id().find(account_id) else {
        return Err("Account id could not be found. If this is unexpected ensure the user has logged into the system once.".into());
    };

    ctx.db
        .tab_project_role_member()
        .try_insert(ProjectRoleMember {
            role_id,
            account_id: user.internal_id,
        })?;

    Ok(())
}

#[reducer]
pub fn project_role_member_remove(
    ctx: &ReducerContext,
    role_id: u32,
    account_id: Uuid,
) -> Result<(), String> {
    let Some(role) = ctx.db.tab_project_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    let request_account_id = ctx.get_user_account()?;
    ctx.auth_builder(role.project_id, request_account_id)?
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    let Some(user) = ctx.db.tab_user().account_id().find(account_id) else {
        return Err("Account id could not be found. If this is unexpected ensure the user has logged into the system once.".into());
    };

    for member in ctx.db.tab_project_role_member().role_id().filter(role_id) {
        if member.account_id == user.internal_id {
            ctx.db.tab_project_role_member().delete(member);
            break;
        }
    }

    Ok(())
}

#[reducer]
pub fn project_role_assign_permission(
    ctx: &ReducerContext,
    role_id: u32,
    new_permissions: u64,
) -> Result<(), String> {
    let Some(mut role) = ctx.db.tab_project_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    let account_id = ctx.get_user_account()?;
    ctx.auth_builder(role.project_id, account_id)?
        //.permission
        .authorize()?;

    role.permissions = new_permissions;

    ctx.db.tab_project_role().id().update(role);

    Ok(())
}

#[reducer]
pub fn project_member_assign_permission(
    ctx: &ReducerContext,
    member_id: u32,
    new_permissions: u64,
) -> Result<(), String> {
    let Some(mut member) = ctx.db.tab_project_member().id().find(member_id) else {
        return Err("Could not find role with id".into());
    };

    let account_id = ctx.get_user_account()?;
    ctx.auth_builder(member.project_id, account_id)?
        //.permission
        .authorize()?;

    member.permissions = new_permissions;

    ctx.db.tab_project_member().id().update(member);

    Ok(())
}
 */
