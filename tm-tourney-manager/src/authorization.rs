use std::{
    ops::{Add, BitAnd, BitOr, Not},
    task::Context,
};

use spacetimedb::{DbContext, ReducerContext, Uuid, ViewContext};

use crate::{
    competition::{
        CompetitionPermissionsV1,
        roles::{
            tab_competition_role, tab_competition_role__view, tab_competition_role_member,
            tab_competition_role_member__view,
        },
    },
    raw_server::{RawServerV1, tab_raw_server, tab_raw_server__view},
    user::{UserV1, tab_user, tab_user__view, tab_user_identity, tab_user_identity__view},
    worker::{TmWorker, tm_worker, tm_worker__view},
};

pub(crate) trait Authorization {
    type Context: DbContext;
    fn get_user(&self) -> Result<UserV1, String>;
    fn get_user_account(&self) -> Result<Uuid, String>;
    fn get_server(&self) -> Result<RawServerV1, String>;
    fn get_worker(&self) -> Result<TmWorker, String>;

    fn auth_builder(
        &self,
        competition_id: u32,
    ) -> AuthBuilder<CompetitionPermissionsV1, Self::Context>;
}

impl Authorization for ReducerContext {
    type Context = ReducerContext;

    fn get_user(&self) -> Result<UserV1, String> {
        let Some(user) = self.db.tab_user_identity().identity().find(self.sender()) else {
            return Err("Identity not associated with a user account.".into());
        };

        let Some(user) = self.db.tab_user().account_id().find(user.account_id) else {
            return Err("AccountId not associated with a user account.".into());
        };

        Ok(user)
    }

    fn get_user_account(&self) -> Result<Uuid, String> {
        let Some(user) = self.db.tab_user_identity().identity().find(self.sender()) else {
            return Err("Identity not associated with a user account.".into());
        };

        Ok(user.account_id)
    }

    fn get_server(&self) -> Result<RawServerV1, String> {
        if let Some(server) = self.db.tab_raw_server().identity().find(self.sender()) {
            return Ok(server);
        }

        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    fn get_worker(&self) -> Result<TmWorker, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.sender()) {
            return Ok(worker);
        }

        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    }

    fn auth_builder(
        &self,
        competition_id: u32,
    ) -> AuthBuilder<CompetitionPermissionsV1, ReducerContext> {
        AuthBuilder::<CompetitionPermissionsV1, ReducerContext>::new(competition_id, self)
    }
}

impl Authorization for ViewContext {
    type Context = ViewContext;

    fn get_user(&self) -> Result<UserV1, String> {
        let Some(user) = self.db.tab_user_identity().identity().find(self.sender()) else {
            return Err("Identity not associated with a user account.".into());
        };

        let Some(user) = self.db.tab_user().account_id().find(user.account_id) else {
            return Err("AccountId not associated with a user account.".into());
        };

        Ok(user)
    }

    fn get_user_account(&self) -> Result<Uuid, String> {
        let Some(user) = self.db.tab_user_identity().identity().find(self.sender()) else {
            return Err("Identity not associated with a user account.".into());
        };

        Ok(user.account_id)
    }

    fn get_server(&self) -> Result<RawServerV1, String> {
        if let Some(server) = self.db.tab_raw_server().identity().find(self.sender()) {
            return Ok(server);
        }

        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    fn get_worker(&self) -> Result<TmWorker, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.sender()) {
            return Ok(worker);
        }

        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    }

    fn auth_builder(
        &self,
        competition_id: u32,
    ) -> AuthBuilder<CompetitionPermissionsV1, ViewContext> {
        AuthBuilder::<CompetitionPermissionsV1, ViewContext>::new(competition_id, self)
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

    fn bypass(&self) -> bool;

    fn passed(self) -> bool;
}
pub(crate) struct AuthBuilder<'a, Item: PermissionType, Ctx: DbContext> {
    //got: Item,
    expected: Item,
    competition_id: u32,
    ctx: &'a Ctx,
}

impl<'a> AuthBuilder<'a, CompetitionPermissionsV1, ReducerContext> {
    fn new(competition_id: u32, ctx: &'a ReducerContext) -> Self {
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

    pub(crate) fn authorize(self) -> Result<Uuid, String> {
        let Some(user) = self
            .ctx
            .db()
            .tab_user_identity()
            .identity()
            .find(self.ctx.sender())
        else {
            return Err("Identity not associated with a user account.".into());
        };
        let permissions = self
            .ctx
            .db
            .tab_competition_role_member()
            .user_roles()
            .filter((self.competition_id, user.account_id))
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
            });
        if (permissions & !self.expected).passed() {
            Ok(user.account_id)
        } else {
            Err("Not sufficient permissions to perform this action.".into())
        }
    }
}

impl<'a> AuthBuilder<'a, CompetitionPermissionsV1, ViewContext> {
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

    pub(crate) fn authorize(self) -> Result<Uuid, String> {
        let Some(user) = self
            .ctx
            .db()
            .tab_user_identity()
            .identity()
            .find(self.ctx.sender())
        else {
            return Err("Identity not associated with a user account.".into());
        };
        let permissions = self
            .ctx
            .db
            .tab_competition_role_member()
            .user_roles()
            .filter((self.competition_id, user.account_id))
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
            });
        if (permissions & !self.expected).passed() {
            Ok(user.account_id)
        } else {
            Err("Not sufficient permissions to perform this action.".into())
        }
    }
}
