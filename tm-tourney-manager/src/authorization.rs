use std::marker::PhantomData;

use spacetimedb::{JwtClaims, ReducerContext, Uuid};

use crate::{
    raw_server::tab_raw_server_online,
    tournament::permissions::{
        TournamentPermissionV1, TournamentPermissionsV1, tab_tournament_permission,
    },
    user::{UserV1, tab_user, user_identity},
    worker::tm_worker,
};

pub(crate) trait Authorization {
    fn get_user(&self) -> Result<UserV1, String>;
    fn get_server(&self) -> Result<String, String>;
    fn get_worker(&self) -> Result<String, String>;

    fn tournament_permissions(
        &self,
        user: &UserV1,
    ) -> AuthBuilder<TournamentPermissionV1, TournamentPermissionsV1>;
}

impl Authorization for ReducerContext {
    fn get_user(&self) -> Result<UserV1, String> {
        let Some(user) = self.db.user_identity().identity().find(self.sender) else {
            return Err("Identity not associated with a user account.".into());
        };

        let Some(user) = self.db.tab_user().account_id().find(user.account_id) else {
            return Err("AccountId not associated with a user account.".into());
        };

        Ok(user)
    }

    fn get_server(&self) -> Result<String, String> {
        if let Some(server) = self.db.tab_raw_server_online().identity().find(self.sender) {
            return Ok(server.tm_login.clone());
        }

        //TODO
        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    fn get_worker(&self) -> Result<String, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.sender) {
            return Ok(worker.tm_login.clone());
        }

        //TODO
        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    }

    fn tournament_permissions(
        &self,
        user: &UserV1,
    ) -> AuthBuilder<TournamentPermissionV1, TournamentPermissionsV1> {
        //self.db.tab_tournament_permission()
        todo!()
    }
}

pub(crate) trait PermissionType {
    fn initial() -> Self;

    fn evaluate(self) -> Result<(), String>;
}
pub(crate) struct AuthBuilder<'a, Base, Item: PermissionType>(&'a Base, Item);

impl<'a, Base, Item: PermissionType> AuthBuilder<'a, Base, Item> {
    fn new(base: &'a Base) -> Self {
        AuthBuilder(base, Item::initial())
    }

    pub(crate) fn permission(self, permission: Item) -> Self {
        //TODO
        self
    }

    pub(crate) fn check(self) -> Result<(), String> {
        self.1.evaluate()
    }
}
