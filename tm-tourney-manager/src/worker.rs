use spacetimedb::{Identity, ProcedureContext, Table};

pub mod jobs;

#[spacetimedb::table(accessor=tm_worker)]
pub struct TmWorker {
    /// Trackmania server logins are unique.
    #[primary_key]
    pub tm_login: String,
    #[unique]
    pub identity: Identity,

    /// Each server also has a ubisoft account associated with it.
    owner_id: String,

    // Whether the server can be reached and has a bridge active.
    online: bool,
}

impl TmWorker {
    pub fn set_identity(&mut self, identity: Identity) {
        self.identity = identity;
    }
}

/// Elevates an annonymous user to a trackmania server.
/// password of the server doesn't get saved but rather verified for validity.
#[spacetimedb::procedure]
pub fn login_as_worker(
    ctx: &mut ProcedureContext,
    login: String,
    password: String,
    account_id: String,
)
/* -> Result<(), String> */
{
    /*  let request = Request::builder()
        .method("POST")
        .uri("https://prod.trackmania.core.nadeo.online/v2/authentication/token/basic")
        .header(
            "Authorization",
            format!(
                "Basic {}",
                BASE64_STANDARD.encode(login.clone() + ":" + &password)
            ),
        )
        .header("Content-Type", "application/json")
        .header("User-Agent", "tm-tourney-manager | central")
        .body(r#"{ "audience": "NadeoServices" }"#)
        .unwrap();
    let result = ctx.http.send(request).unwrap();

    let status = result.status();

    if status.is_success() {
        let body = result.into_body();
        let string = body.into_string().unwrap();
        //let string = BASE64_STANDARD.decode(string);
        log::error!("{:?}", string)
    } else {
        //TODO error handling
        log::error!("Server registration failed because of nadeo request");
        panic!()
    } */

    let sender = ctx.sender();

    ctx.with_tx(|ctx| {
        if ctx.db.tm_worker().identity().find(sender).is_some() {
            // Server identity is already verified.
            // return Ok(());
        }
        if let Some(mut worker) = ctx.db.tm_worker().tm_login().find(&login) {
            // The new identity is assigned to the server.
            log::error!("Setting new identity {}", sender);
            worker.set_identity(sender);
            ctx.db.tm_worker().tm_login().update(worker);
            //Ok(())
        } else {
            //TODO make HTTP call when its available and verify that credentials are correct.

            // Server has never been seen before so create a new one.
            ctx.db.tm_worker().insert(TmWorker {
                online: true,
                tm_login: login.clone(),
                owner_id: account_id.clone(),
                identity: sender,
            });
            //Ok(())
        }
    });
}
