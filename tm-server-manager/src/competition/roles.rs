use spacetimedb::{ReducerContext, Table, Uuid, reducer, table};

use crate::{authorization::Authorization, competition::CompetitionPermissionsV1, user::UserRead};

#[table(accessor= tab_competition_role)]
pub struct CompetitionRole {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    #[index(hash)]
    competition_id: u32,

    permissions: u64,
}

impl CompetitionRole {
    pub(crate) fn get_permissions1(&self) -> CompetitionPermissionsV1 {
        CompetitionPermissionsV1(self.permissions)
    }
}

#[table(accessor= tab_competition_role_member,index(accessor= user_roles , hash(columns= [role_id,user_id])))]
pub struct CompetitionRoleMember {
    #[index(hash)]
    role_id: u32,

    #[index(hash)]
    user_id: u32,
}

impl CompetitionRoleMember {
    pub fn get_role_id(&self) -> u32 {
        self.role_id
    }
}

#[table(accessor= tab_competition_member,index(accessor= user_roles , hash(columns= [competition_id,user_id])))]
pub struct CompetitionMember {
    permissions: u64,

    #[auto_inc]
    #[primary_key]
    id: u32,

    #[index(hash)]
    competition_id: u32,

    #[index(hash)]
    user_id: u32,
}

#[reducer]
pub fn member_add(
    ctx: &ReducerContext,
    competition_id: u32,
    account_id: Uuid,
) -> Result<(), String> {
    let user_id = ctx
        .auth_builder(competition_id)
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db
        .tab_competition_member()
        .try_insert(CompetitionMember {
            competition_id,
            permissions: 0,
            id: 0,
            user_id: ctx.user_id_from_account(account_id),
        })?;

    Ok(())
}

#[reducer]
pub fn member_remove(ctx: &ReducerContext, member_id: u32) -> Result<(), String> {
    let Some(project_member) = ctx.db.tab_competition_member().id().find(member_id) else {
        return Err("Member with id not found!".into());
    };

    ctx.auth_builder(project_member.competition_id)
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db
        .tab_competition_member()
        .id()
        .delete(project_member.id);

    Ok(())
}

#[reducer]
pub fn role_create(ctx: &ReducerContext, competition_id: u32, name: String) -> Result<(), String> {
    ctx.auth_builder(competition_id)
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db.tab_competition_role().try_insert(CompetitionRole {
        name,
        id: 0,
        competition_id,
        permissions: 0,
    })?;

    Ok(())
}

#[reducer]
pub fn role_remove(ctx: &ReducerContext, role_id: u32) -> Result<(), String> {
    let Some(role) = ctx.db.tab_competition_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    //TODO
    ctx.auth_builder(role.competition_id)
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db.tab_competition_role().id().delete(role_id);

    ctx.db
        .tab_competition_role_member()
        .role_id()
        .delete(role_id);

    Ok(())
}

#[reducer]
pub fn role_member_assign(
    ctx: &ReducerContext,
    role_id: u32,
    account_id: Uuid,
) -> Result<(), String> {
    let Some(role) = ctx.db.tab_competition_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    //TODO
    ctx.auth_builder(role.competition_id)
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    ctx.db
        .tab_competition_role_member()
        .try_insert(CompetitionRoleMember {
            role_id,
            user_id: ctx.user_id_from_account(account_id),
        })?;

    Ok(())
}

#[reducer]
pub fn role_member_remove(
    ctx: &ReducerContext,
    role_id: u32,
    account_id: Uuid,
) -> Result<(), String> {
    let Some(role) = ctx.db.tab_competition_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    ctx.auth_builder(role.competition_id)
        //.permission(ProjectPermissionsV1::Pro)
        .authorize()?;

    for member in ctx
        .db
        .tab_competition_role_member()
        .role_id()
        .filter(role_id)
    {
        if member.user_id == ctx.user_id_from_account(account_id) {
            ctx.db.tab_competition_role_member().delete(member);
            break;
        }
    }

    Ok(())
}

#[reducer]
pub fn role_assign_permission(
    ctx: &ReducerContext,
    role_id: u32,
    new_permissions: u64,
) -> Result<(), String> {
    let Some(mut role) = ctx.db.tab_competition_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    ctx.auth_builder(role.competition_id)
        //.permission
        .authorize()?;

    role.permissions = new_permissions;

    ctx.db.tab_competition_role().id().update(role);

    Ok(())
}

#[reducer]
pub fn member_assign_permission(
    ctx: &ReducerContext,
    member_id: u32,
    new_permissions: u64,
) -> Result<(), String> {
    let Some(mut member) = ctx.db.tab_competition_member().id().find(member_id) else {
        return Err("Could not find role with id".into());
    };

    ctx.auth_builder(member.competition_id)
        //.permission
        .authorize()?;

    member.permissions = new_permissions;

    ctx.db.tab_competition_member().id().update(member);

    Ok(())
}
