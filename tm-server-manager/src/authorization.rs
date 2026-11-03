use std::ops::{Add, BitAnd, BitOr, Not};

use spacetimedb::{CtxDbRead, CtxWithSender, Identity};

use crate::{
    competition::{CompetitionPermissionsV1, roles::CompetitionPermissionRead},
    raw_server::TabRawServerRead,
    user::UserRead,
};

pub(crate) trait Authorization<Db: CtxWithSender + CtxDbRead> {
    fn user_id(&self) -> Result<u32, String>;

    fn server_id(&self) -> Result<u32, String>;

    fn auth_builder(&'_ self, competition_id: u32)
    -> AuthBuilder<'_, CompetitionPermissionsV1, Db>;
}

impl<Db: CtxWithSender + CtxDbRead> Authorization<Db> for Db {
    fn server_id(&self) -> Result<u32, String> {
        self.get_raw_server_id(self.sender())
    }

    fn auth_builder(
        &'_ self,
        competition_id: u32,
    ) -> AuthBuilder<'_, CompetitionPermissionsV1, Db> {
        AuthBuilder::<CompetitionPermissionsV1, Db>::new(competition_id, self, self.sender())
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
    expected: Item,
    competition_id: u32,
    db: &'a Db,
    sender: Identity,
}

impl<'a, Db: CtxDbRead> AuthBuilder<'a, CompetitionPermissionsV1, Db> {
    fn new(competition_id: u32, db: &'a Db, sender: Identity) -> Self {
        AuthBuilder {
            expected: CompetitionPermissionsV1::initial(),
            competition_id,
            db,
            sender,
        }
    }

    pub(crate) fn permission(mut self, permission: CompetitionPermissionsV1) -> Self {
        self.expected = self.expected | permission;
        self
    }

    pub(crate) fn authorize(self) -> Result<u32, String> {
        let user_id = self.db.get_user_id(self.sender)?;

        // Membership is always required, even when no permission is expected.
        match self.db.competition_access(self.competition_id, user_id) {
            None => Err("You are not a member of this competition.".into()),
            Some(permissions) if permissions.has(self.expected) => Ok(user_id),
            Some(_) => Err("Not sufficient permissions to perform this action.".into()),
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
