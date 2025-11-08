use spacetimedb::{JwtClaims, ReducerContext};

use crate::server::tm_server;

pub trait Authorization {
    fn auth_user(&self) -> Result<String, String>;
    fn auth_server(&self) -> Result<String, String>;
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
            return Ok(server.id.clone());
        }

        //TODO
        Err("Tried to use a reducer meant for Servers without the proper Authentication.".into())
    }
}
