use spacetimedb::{JwtClaims, ReducerContext, Uuid};

use crate::{raw_server::tab_raw_server_online, user::user_identity, worker::tm_worker};

pub trait Authorization {
    fn get_user(&self) -> Result<Uuid, String>;
    fn get_server(&self) -> Result<String, String>;
    fn get_worker(&self) -> Result<String, String>;
}

impl Authorization for ReducerContext {
    fn get_user(&self) -> Result<Uuid, String> {
        let Some(user) = self.db.user_identity().identity().find(self.sender) else {
            return Err("Identity not associated with a user account.".into());
        };

        Ok(user.account_id)
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
}
