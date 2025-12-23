use spacetimedb::{JwtClaims, ReducerContext};

use crate::{server::tm_server, worker::tm_worker};

pub trait Authorization {
    fn auth_user(&self) -> Result<String, String>;
    fn auth_server(&self) -> Result<String, String>;
    fn auth_worker(&self) -> Result<String, String>;
}

impl Authorization for ReducerContext {
    fn auth_user(&self) -> Result<String, String> {
        if let Some(jwt) = self.sender_auth().jwt() {
            //TODO get the ubi id claim.
            Ok(jwt.subject().into())
        } else {
            Err(
                "User tried to use a reducer without the proper Authentication. JWT missing!"
                    .into(),
            )
        }
    }

    fn auth_server(&self) -> Result<String, String> {
        if let Some(server) = self.db.tm_server().identity().find(self.identity()) {
            return Ok(server.tm_login.clone());
        }

        //TODO
        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }

    fn auth_worker(&self) -> Result<String, String> {
        if let Some(worker) = self.db.tm_worker().identity().find(self.identity()) {
            return Ok(worker.tm_login.clone());
        }

        //TODO
        Err("Tried to use a reducer meant for Workers without the proper Authentication.".into())
    }
}
