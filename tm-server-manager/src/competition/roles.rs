use spacetimedb::{AnonymousViewContext, Query, ReducerContext, Table, Uuid, reducer, table, view};

use crate::{authorization::Authorization, competition::CompetitionPermissionsV1, user::UserRead};

//TODO make private again
#[table(accessor= tab_competition_role,public)]
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

//TODO make private again
#[table(accessor= tab_competition_role_member,index(accessor= user_roles , hash(columns= [role_id,user_id])),public)]
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

//TODO make private again
#[table(accessor= tab_competition_member,index(accessor= user_member , hash(columns= [competition_id,user_id])),public)]
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

impl CompetitionMember {
    pub(crate) fn new_owner(user_id: u32, competition_id: u32) -> Self {
        CompetitionMember {
            permissions: CompetitionPermissionsV1::OWNER.0,
            id: 0,
            competition_id,
            user_id,
        }
    }

    pub(crate) fn get_permissions(&self) -> CompetitionPermissionsV1 {
        CompetitionPermissionsV1(self.permissions)
    }

    pub(crate) fn user(&self) -> u32 {
        self.user_id
    }
}

#[reducer]
fn member_add(ctx: &ReducerContext, competition_id: u32, account_id: Uuid) -> Result<(), String> {
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
fn member_remove(ctx: &ReducerContext, member_id: u32) -> Result<(), String> {
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

    //TODO
    // ctx.db.tab_competition_role_member().user_roles().filter((point))

    Ok(())
}

#[reducer]
fn role_create(ctx: &ReducerContext, competition_id: u32, name: String) -> Result<(), String> {
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
fn role_remove(ctx: &ReducerContext, role_id: u32) -> Result<(), String> {
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
fn role_member_assign(ctx: &ReducerContext, role_id: u32, account_id: Uuid) -> Result<(), String> {
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
fn role_member_remove(ctx: &ReducerContext, role_id: u32, account_id: Uuid) -> Result<(), String> {
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
fn role_assign_permission(
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
fn member_assign_permission(
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

/* #[view(accessor=unstable_competition_members,public)]
fn unstable_competition_members(ctx: &AnonymousViewContext) -> impl Query<CompetitionMember> {
    ctx.from.tab_competition_member()
} */

/* #[view(accessor=unstable_competition_role,public)]
fn unstable_competition_role(ctx: &AnonymousViewContext) -> impl Query<CompetitionRole> {
    ctx.from.tab_competition_role()
}

#[view(accessor=unstable_competition_role_member,public)]
fn unstable_competition_role_member(
    ctx: &AnonymousViewContext,
) -> impl Query<CompetitionRoleMember> {
    ctx.from.tab_competition_role_member()
} */

/* #[view(accessor=my_competition_members,public)]
fn my_competition_members(ctx: &AnonymousViewContext) -> impl Query<CompetitionMember> {
    let competition_id = 1u32;
    ctx.from
        .tab_competition_member()
        .r#where(|r| r.competition_id.eq(competition_id))
}

#[view(accessor=my_competition_role,public)]
fn my_competition_role(ctx: &AnonymousViewContext) -> impl Query<CompetitionRole> {
    let competition_id = 1u32;
    ctx.from
        .tab_competition_role()
        .r#where(|r| r.competition_id.eq(competition_id))
}

#[view(accessor=my_competition_role_member,public)]
fn my_competition_role_member(ctx: &AnonymousViewContext) -> impl Query<CompetitionRoleMember> {
    let competition_id = 1u32;
    /* ctx.from
        .tab_competition_role()
        .r#where(|r| r.competition_id.eq(competition_id))
        .right_semijoin(ctx.from.tab_competition_role_member(), |a, b| {
            b.role_id.eq(a.id)
        }) */
       ctx.from.
} */
