use spacetimedb::{
    CtxDbRead, ReducerContext, SpacetimeType, Table, Uuid, ViewContext, reducer, table, view,
};

use crate::{
    authorization::Authorization,
    competition::{CompetitionPermissionsV1, CompetitionRead},
    user::UserRead,
};

#[table(accessor= tab_competition_role, vis_private)]
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

#[table(accessor= tab_competition_role_member, vis_private,index(accessor= user_roles , hash(columns= [role_id,user_id])))]
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

#[table(accessor= tab_competition_member, vis_private,index(accessor= user_member , hash(columns= [competition_id,user_id])))]
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

pub(crate) trait CompetitionPermissionRead {
    /// The permissions of a user in a competition, given directly or through roles, including the competitions above.
    /// `None` if the user is not a member of the competition or of one above it.
    fn competition_access(
        &self,
        competition_id: u32,
        user_id: u32,
    ) -> Option<CompetitionPermissionsV1>;
    /// The permissions of a user in a competition, `NONE` for non-members.
    fn competition_permissions(
        &self,
        competition_id: u32,
        user_id: u32,
    ) -> CompetitionPermissionsV1;
    /// Whether the user is a member of the competition or of one above it, with or without permissions.
    fn competition_is_member(&self, competition_id: u32, user_id: u32) -> bool;

    fn check_grant(
        &self,
        competition_id: u32,
        user_id: u32,
        old: u64,
        new: u64,
    ) -> Result<(), String>;
}

impl<Db: CtxDbRead> CompetitionPermissionRead for Db {
    fn competition_access(
        &self,
        competition_id: u32,
        user_id: u32,
    ) -> Option<CompetitionPermissionsV1> {
        let db = self.db_read_only();
        let mut is_member = false;
        let permissions = self.competition_ancestors(competition_id).into_iter().fold(
            CompetitionPermissionsV1::NONE,
            |acc, id| {
                let direct = db
                    .tab_competition_member()
                    .user_member()
                    .filter((id, user_id))
                    .fold(acc, |acc, member| {
                        is_member = true;
                        acc | member.get_permissions()
                    });
                db.tab_competition_role()
                    .competition_id()
                    .filter(id)
                    .filter(|role| {
                        db.tab_competition_role_member()
                            .user_roles()
                            .filter((role.id, user_id))
                            .next()
                            .is_some()
                    })
                    .fold(direct, |acc, role| acc | role.get_permissions1())
            },
        );
        // Roles left over from a removed membership must not grant anything.
        is_member.then_some(permissions)
    }

    fn competition_permissions(
        &self,
        competition_id: u32,
        user_id: u32,
    ) -> CompetitionPermissionsV1 {
        self.competition_access(competition_id, user_id)
            .unwrap_or(CompetitionPermissionsV1::NONE)
    }

    fn competition_is_member(&self, competition_id: u32, user_id: u32) -> bool {
        self.competition_access(competition_id, user_id).is_some()
    }

    /// Only the bits a user holds can be granted or revoked by them, owners can change everything.
    fn check_grant(
        &self,
        competition_id: u32,
        user_id: u32,
        old: u64,
        new: u64,
    ) -> Result<(), String> {
        if self
            .competition_permissions(competition_id, user_id)
            .has(CompetitionPermissionsV1(old ^ new))
        {
            Ok(())
        } else {
            Err("You can only grant or revoke permissions you have yourself.".into())
        }
    }
}

pub(crate) trait CompetitionMemberRead {
    /// The direct members of the competition, the ones of the competitions above are not included.
    fn competition_members(&self, competition_id: u32) -> Vec<CompetitionMember>;
}
impl<Db: CtxDbRead> CompetitionMemberRead for Db {
    fn competition_members(&self, competition_id: u32) -> Vec<CompetitionMember> {
        self.db_read_only()
            .tab_competition_member()
            .competition_id()
            .filter(competition_id)
            .collect()
    }
}

pub(crate) trait CompetitionMemberWrite: CompetitionMemberRead {
    fn competition_member_insert(&self, member: CompetitionMember) -> Result<(), String>;
}
impl<Db: spacetimedb::CtxDbWrite> CompetitionMemberWrite for Db {
    fn competition_member_insert(&self, member: CompetitionMember) -> Result<(), String> {
        self.db().tab_competition_member().try_insert(member)?;
        Ok(())
    }
}

#[reducer]
fn member_add(ctx: &ReducerContext, competition_id: u32, account_id: Uuid) -> Result<(), String> {
    ctx.auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_MEMBER_ADD)
        .authorize()?;

    let user_id = ctx.user_id_from_account(account_id);
    if ctx
        .db
        .tab_competition_member()
        .user_member()
        .filter((competition_id, user_id))
        .next()
        .is_some()
    {
        return Err("The user is already a member.".into());
    }

    ctx.db
        .tab_competition_member()
        .try_insert(CompetitionMember {
            competition_id,
            permissions: 0,
            id: 0,
            user_id,
        })?;

    Ok(())
}

#[reducer]
fn member_remove(ctx: &ReducerContext, member_id: u32) -> Result<(), String> {
    let Some(member) = ctx.db.tab_competition_member().id().find(member_id) else {
        return Err("Member with id not found!".into());
    };

    ctx.auth_builder(member.competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_MEMBER_REMOVE)
        .authorize()?;

    ctx.db.tab_competition_member().id().delete(member.id);

    // The roles of the competition only make sense for its members.
    for role in ctx
        .db
        .tab_competition_role()
        .competition_id()
        .filter(member.competition_id)
    {
        ctx.db
            .tab_competition_role_member()
            .user_roles()
            .delete((role.id, member.user_id));
    }

    Ok(())
}

#[reducer]
fn role_create(ctx: &ReducerContext, competition_id: u32, name: String) -> Result<(), String> {
    ctx.auth_builder(competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_ROLE_CREATE)
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

    ctx.auth_builder(role.competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_ROLE_REMOVE)
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

    ctx.auth_builder(role.competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_ROLE_MEMBER_ASSIGN)
        .authorize()?;

    let user_id = ctx.user_id_from_account(account_id);
    if !ctx.competition_is_member(role.competition_id, user_id) {
        return Err("Only members of the competition can be given a role.".into());
    }
    if ctx
        .db
        .tab_competition_role_member()
        .user_roles()
        .filter((role_id, user_id))
        .next()
        .is_some()
    {
        return Err("The user already has this role.".into());
    }

    ctx.db
        .tab_competition_role_member()
        .try_insert(CompetitionRoleMember { role_id, user_id })?;

    Ok(())
}

#[reducer]
fn role_member_remove(ctx: &ReducerContext, role_id: u32, account_id: Uuid) -> Result<(), String> {
    let Some(role) = ctx.db.tab_competition_role().id().find(role_id) else {
        return Err("Could not find role with id".into());
    };

    ctx.auth_builder(role.competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_ROLE_MEMBER_REMOVE)
        .authorize()?;

    let user_id = ctx.user_id_from_account(account_id);
    ctx.db
        .tab_competition_role_member()
        .user_roles()
        .delete((role_id, user_id));

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

    let user_id = ctx
        .auth_builder(role.competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_ROLE_EDIT_PERMISSION)
        .authorize()?;
    ctx.check_grant(
        role.competition_id,
        user_id,
        role.permissions,
        new_permissions,
    )?;

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
        return Err("Could not find member with id".into());
    };

    let user_id = ctx
        .auth_builder(member.competition_id)
        .permission(CompetitionPermissionsV1::PERMISSION_MEMBER_EDIT_PERMISSION)
        .authorize()?;
    ctx.check_grant(
        member.competition_id,
        user_id,
        member.permissions,
        new_permissions,
    )?;

    member.permissions = new_permissions;

    ctx.db.tab_competition_member().id().update(member);

    Ok(())
}

// Members, roles and permissions of a competition are only visible to its members and the ones above.

#[view(accessor = competition_members, public)]
fn competition_members(ctx: &ViewContext, competition_id: u32) -> Vec<CompetitionMember> {
    if ctx.auth_builder(competition_id).authorize().is_err() {
        return Vec::new();
    }
    ctx.db
        .tab_competition_member()
        .competition_id()
        .filter(competition_id)
        .collect()
}

#[view(accessor = competition_roles, public)]
fn competition_roles(ctx: &ViewContext, competition_id: u32) -> Vec<CompetitionRole> {
    if ctx.auth_builder(competition_id).authorize().is_err() {
        return Vec::new();
    }
    ctx.db
        .tab_competition_role()
        .competition_id()
        .filter(competition_id)
        .collect()
}

#[view(accessor = competition_role_members, public)]
fn competition_role_members(ctx: &ViewContext, competition_id: u32) -> Vec<CompetitionRoleMember> {
    if ctx.auth_builder(competition_id).authorize().is_err() {
        return Vec::new();
    }
    ctx.db
        .tab_competition_role()
        .competition_id()
        .filter(competition_id)
        .flat_map(|role| {
            ctx.db
                .tab_competition_role_member()
                .role_id()
                .filter(role.id)
        })
        .collect()
}

#[derive(Debug, SpacetimeType)]
pub struct MyCompetitionPermissions {
    pub competition_id: u32,
    pub permissions: u64,
}

/// The permissions of the viewer, so clients can tell what the user is allowed to do.
#[view(accessor = my_competition_permissions, public)]
fn my_competition_permissions(
    ctx: &ViewContext,
    competition_id: u32,
) -> Option<MyCompetitionPermissions> {
    let user_id = ctx.user_id().ok()?;
    Some(MyCompetitionPermissions {
        competition_id,
        permissions: ctx.competition_permissions(competition_id, user_id).0,
    })
}
