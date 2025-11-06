use spacetimedb::{JwtClaims, ReducerContext};

pub trait Authorization {
    fn is_authorized(&self) -> Result<(), String>;
}

impl Authorization for ReducerContext {
    fn is_authorized(&self) -> Result<(), String> {
        if let Some(jwt) = self.sender_auth().jwt() {
            Ok(())
        } else {
            Err(
                "User tried to use a reducer without the proper Authentication. JWT missing!"
                    .into(),
            )
        }
    }
}
