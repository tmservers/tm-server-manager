use spacetimedb::{JwtClaims, ReducerContext};

pub trait Authorization {
    fn auth(&self) -> Result<String, String>;
}

impl Authorization for ReducerContext {
    fn auth(&self) -> Result<String, String> {
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
}
