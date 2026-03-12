use std::ops::{Add, BitAnd, BitOr, Not};

use spacetimedb::{ReducerContext, Uuid, ViewContext};

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
    fn get_user(&self) -> Result<UserV1, String>;
    fn get_user_account(&self) -> Result<Uuid, String>;
    fn get_server(&self) -> Result<RawServerV1, String>;
    fn get_worker(&self) -> Result<TmWorker, String>;

    fn auth_builder(
        &self,
        project_id: u32,
        account_id: Uuid,
    ) -> Result<AuthBuilder<CompetitionPermissionsV1>, String>;
}

impl Authorization for ReducerContext {
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
        account_id: Uuid,
    ) -> Result<AuthBuilder<CompetitionPermissionsV1>, String> {
        let permissions = self
            .db
            .tab_competition_role_member()
            .user_roles()
            .filter((competition_id, account_id))
            .fold(CompetitionPermissionsV1::default(), |acc, member| {
                if let Some(role) = self
                    .db
                    .tab_competition_role()
                    .id()
                    .find(member.get_role_id())
                {
                    return acc | role.get_permissions1();
                }
                acc
            });
        Ok(AuthBuilder::new(permissions))
    }
}

impl Authorization for ViewContext {
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

        //TODO
        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    fn get_worker(&self) -> Result<TmWorker, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.sender()) {
            return Ok(worker);
        }

        //TODO
        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    }

    fn auth_builder(
        &self,
        competition_id: u32,
        account_id: Uuid,
    ) -> Result<AuthBuilder<CompetitionPermissionsV1>, String> {
        let permissions = self
            .db
            .tab_competition_role_member()
            .user_roles()
            .filter((competition_id, account_id))
            .fold(CompetitionPermissionsV1::default(), |acc, member| {
                if let Some(role) = self
                    .db
                    .tab_competition_role()
                    .id()
                    .find(member.get_role_id())
                {
                    return acc | role.get_permissions1();
                }
                acc
            });
        Ok(AuthBuilder::new(permissions))
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
pub(crate) struct AuthBuilder<Item: PermissionType> {
    got: Item,
    expected: Item,
}

impl<Item: PermissionType> AuthBuilder<Item> {
    fn new(got: Item) -> Self {
        AuthBuilder {
            got,
            expected: Item::initial(),
        }
    }

    pub(crate) fn permission(mut self, permission: Item) -> Self {
        self.expected = self.expected | permission;
        self
    }

    pub(crate) fn authorize(self) -> Result<(), String> {
        if (self.got & !self.expected).passed() {
            Ok(())
        } else {
            Err("Not sufficient permissions to perform this action.".into())
        }
    }
}
