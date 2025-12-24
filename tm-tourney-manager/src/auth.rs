use spacetimedb::{JwtClaims, ReducerContext};

use crate::{server::tm_server, user::user_identity, worker::tm_worker};

pub trait Authorization {
    fn is_user(&self) -> Result<String, String>;
    fn is_server(&self) -> Result<String, String>;
    fn is_worker(&self) -> Result<String, String>;
}

impl Authorization for ReducerContext {
    fn is_user(&self) -> Result<String, String> {
        let Some(user) = self.db.user_identity().identity().find(self.identity()) else {
            return Err("Identity not associated with a user account.".into());
        };

        Ok(user.account_id)
    }

    fn is_server(&self) -> Result<String, String> {
        if let Some(server) = self.db.tm_server().identity().find(self.identity()) {
            return Ok(server.tm_login.clone());
        }

        //TODO
        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    fn is_worker(&self) -> Result<String, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.identity()) {
            return Ok(worker.tm_login.clone());
        }

        //TODO
        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    }
}
