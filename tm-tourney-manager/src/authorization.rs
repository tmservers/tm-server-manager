use std::{
    marker::PhantomData,
    ops::{Add, BitAnd, Neg, Not},
};

use spacetimedb::{JwtClaims, ReducerContext, Uuid};

use crate::{
    raw_server::tab_raw_server,
    tournament::{
        permissions::{TournamentPermissionV1, TournamentPermissionsV1, tab_tournament_permission},
        tournament,
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
        tournament_id: u32,
        user: &UserV1,
    ) -> Result<AuthBuilder<TournamentPermissionsV1>, String>;
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
        if let Some(server) = self.db.tab_raw_server().identity().find(self.sender) {
            return Ok(server.server_login.clone());
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
        tournament_id: u32,
        user: &UserV1,
    ) -> Result<AuthBuilder<TournamentPermissionsV1>, String> {
        let Some(tournament) = self
            .db
            .tab_tournament_permission()
            .account_and_tournament()
            .filter((user.account_id, tournament_id))
            .next()
        else {
            return Err("Tournament Permission entry could not be found!".into());
        };
        Ok(AuthBuilder::new(tournament.get_permissions()))
    }
}

pub(crate) trait PermissionType:
    Add<Output = Self> + std::marker::Sized + Eq + Copy + BitAnd<Output = Self> + Not<Output = Self>
{
    fn initial() -> Self;

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
        self.expected = self.expected + permission;
        self
    }

    pub(crate) fn check(self) -> Result<(), String> {
        if (self.got & !self.expected).passed() {
            Ok(())
        } else {
            Err("Not sufficient permissions to perform this action.".into())
        }
    }
}
