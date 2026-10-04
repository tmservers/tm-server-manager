use std::ops::{Add, BitAnd, BitOr, Not};

use spacetimedb::{CtxDbRead, CtxWithSender, Identity};

use crate::{
    competition::{
        CompetitionPermissionsV1, CompetitionRead,
        roles::{
            tab_competition_member__view, tab_competition_role__view,
            tab_competition_role_member__view,
        },
    },
    raw_server::TabRawServerRead,
    user::UserRead,
};

pub(crate) trait Authorization<T: CtxWithSender + CtxDbRead> {
    fn user_id(&self) -> Result<u32, String>;

    fn server_id(&self) -> Result<u32, String>;

    fn auth_builder(&'_ self, competition_id: u32) -> AuthBuilder<'_, CompetitionPermissionsV1, T>;
}

impl<T: CtxWithSender + CtxDbRead> Authorization<T> for T {
    fn server_id(&self) -> Result<u32, String> {
        self.get_raw_server_id(self.sender())
    }

    fn auth_builder(&'_ self, competition_id: u32) -> AuthBuilder<'_, CompetitionPermissionsV1, T> {
        AuthBuilder::<CompetitionPermissionsV1, T>::new(competition_id, self, self.sender())
    }

    fn user_id(&self) -> Result<u32, String> {
        self.get_user_id(self.sender())
    }
}

pub(crate) trait PermissionType:
    Add<Output = Self>
    + std::marker::Sized
    + Eq
    + Copy
    + BitAnd<Output = Self>
    + Not<Output = Self>
    + BitOr<Output = Self>
{
    fn initial() -> Self;

    fn bypass(self) -> bool;
}
pub(crate) struct AuthBuilder<'a, Item: PermissionType, Db: CtxDbRead> {
    //got: Item,
    expected: Item,
    competition_id: u32,
    ctx: &'a Db,
    sender: Identity,
}

impl<'a, Db: CtxDbRead> AuthBuilder<'a, CompetitionPermissionsV1, Db> {
    fn new(competition_id: u32, ctx: &'a Db, sender: Identity) -> Self {
        AuthBuilder {
            expected: CompetitionPermissionsV1::initial(),
            competition_id,
            ctx,
            sender,
        }
    }

    pub(crate) fn permission(mut self, permission: CompetitionPermissionsV1) -> Self {
        self.expected = self.expected | permission;
        self
    }

    pub(crate) fn authorize(self) -> Result<u32, String> {
        let user_id = self.ctx.get_user_id(self.sender)?;

        let tree = self.ctx.competition_ancestors(self.competition_id);
        let mut permissions = Vec::new();
        for competition_id in tree {
            permissions.push(
                self.ctx
                    .db_read_only()
                    .tab_competition_role_member()
                    .user_roles()
                    .filter((competition_id, user_id))
                    .fold(CompetitionPermissionsV1::default(), |acc, member| {
                        if let Some(role) = self
                            .ctx
                            .db_read_only()
                            .tab_competition_role()
                            .id()
                            .find(member.get_role_id())
                        {
                            return acc | role.get_permissions1();
                        }
                        acc
                    }),
            );
            permissions.push(
                self.ctx
                    .db_read_only()
                    .tab_competition_member()
                    .user_member()
                    .filter((competition_id, user_id))
                    .fold(CompetitionPermissionsV1::default(), |acc, member| {
                        acc | member.get_permissions()
                    }),
            );
        }
        let permissions = permissions
            .into_iter()
            .fold(CompetitionPermissionsV1::default(), |acc, acc2| acc | acc2);

        if permissions.bypass() || (self.expected & !permissions) == CompetitionPermissionsV1::NONE
        {
            Ok(user_id)
        } else {
            Err("Not sufficient permissions to perform this action.".into())
        }
    }
}

/* impl<'a> AuthBuilder<'a, CompetitionPermissionsV1, ViewContext> {
    fn new(competition_id: u32, ctx: &'a ViewContext) -> Self {
        AuthBuilder {
            expected: CompetitionPermissionsV1::initial(),
            competition_id,
            ctx,
        }
    }

    pub(crate) fn permission(mut self, permission: CompetitionPermissionsV1) -> Self {
        self.expected = self.expected | permission;
        self
    }

    pub(crate) fn authorize(self) -> Result<u32, String> {
        let user_id = self.ctx.get_user_id(self.ctx.sender())?;

        let tree = self.ctx.competition_ancestors(self.competition_id);
        let mut permissions = Vec::new();
        for competition_id in tree {
            permissions.push(
                self.ctx
                    .db()
                    .tab_competition_role_member()
                    .user_roles()
                    .filter((competition_id, user_id))
                    .fold(CompetitionPermissionsV1::default(), |acc, member| {
                        if let Some(role) = self
                            .ctx
                            .db
                            .tab_competition_role()
                            .id()
                            .find(member.get_role_id())
                        {
                            return acc | role.get_permissions1();
                        }
                        acc
                    }),
            );
            permissions.push(
                self.ctx
                    .db()
                    .tab_competition_member()
                    .user_member()
                    .filter((competition_id, user_id))
                    .fold(CompetitionPermissionsV1::default(), |acc, member| {
                        acc | member.get_permissions()
                    }),
            );
        }
        let permissions = permissions
            .into_iter()
            .fold(CompetitionPermissionsV1::default(), |acc, acc2| acc | acc2);

        if permissions.bypass() || (self.expected & !permissions) == CompetitionPermissionsV1::NONE
        {
            Ok(user_id)
        } else {
            Err("Not sufficient permissions to perform this action.".into())
        }
    }
} */
