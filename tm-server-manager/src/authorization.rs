use std::ops::{Add, BitAnd, BitOr, Not};

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
    user::{UserRead, UserV1},
};

pub(crate) trait Authorization {
    type Context: DbContext;
    // fn get_user(&self) -> Result<UserV1, String>;
    //fn user_account_id(&self) -> Result<Uuid, String>;
    fn user_id(&self) -> Result<u32, String>;

    fn get_server(&self) -> Result<RawServerV1, String>;
    //fn get_worker(&self) -> Result<TmWorker, String>;

    fn auth_builder(
        &'_ self,
        competition_id: u32,
    ) -> AuthBuilder<'_, CompetitionPermissionsV1, Self::Context>;
}

impl Authorization for ReducerContext {
    type Context = ReducerContext;

    /* fn get_user(&self) -> Result<UserV1, String> {
        self.user(self.sender())
    } */

    /*  fn user_account_id(&self) -> Result<u32, String> {
        self.get_user_account_id(self.sender())
    } */

    fn get_server(&self) -> Result<RawServerV1, String> {
        if let Some(server) = self.db.tab_raw_server().identity().find(self.sender()) {
            return Ok(server);
        }

        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    /* fn get_worker(&self) -> Result<TmWorker, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.sender()) {
            return Ok(worker);
        }

        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    } */

    fn auth_builder(
        &'_ self,
        competition_id: u32,
    ) -> AuthBuilder<'_, CompetitionPermissionsV1, ReducerContext> {
        AuthBuilder::<CompetitionPermissionsV1, ReducerContext>::new(competition_id, self)
    }

    fn user_id(&self) -> Result<u32, String> {
        self.get_user_id(self.sender())
    }
}

impl Authorization for ViewContext {
    type Context = ViewContext;

    /* fn get_user(&self) -> Result<UserV1, String> {
        self.user(self.sender())
    } */

    fn user_id(&self) -> Result<u32, String> {
        self.get_user_id(self.sender())
    }

    fn get_server(&self) -> Result<RawServerV1, String> {
        if let Some(server) = self.db.tab_raw_server().identity().find(self.sender()) {
            return Ok(server);
        }

        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    /* fn get_worker(&self) -> Result<TmWorker, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.sender()) {
            return Ok(worker);
        }

        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    } */

    fn auth_builder(
        &'_ self,
        competition_id: u32,
    ) -> AuthBuilder<'_, CompetitionPermissionsV1, ViewContext> {
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

    pub(crate) fn authorize(self) -> Result<u32, String> {
        let user_id = self.ctx.get_user_id(self.ctx.sender())?;
        //TODO inherit permissions from the whole competition tree. Also accumulate the user permissions on top of the role ones.
        let permissions = self
            .ctx
            .db()
            .tab_competition_role_member()
            .user_roles()
            .filter((self.competition_id, user_id))
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
            Ok(user_id)
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

    pub(crate) fn authorize(self) -> Result<u32, String> {
        let user_id = self.ctx.get_user_id(self.ctx.sender())?;
        let permissions = self
            .ctx
            .db
            .tab_competition_role_member()
            .user_roles()
            .filter((self.competition_id, user_id))
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
            Ok(user_id)
        } else {
            Err("Not sufficient permissions to perform this action.".into())
        }
    }
}
