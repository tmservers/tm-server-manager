use std::rc::Rc;

use leptos::prelude::*;
use singlestage::{Button, card::*};

use crate::generated::DbConnection;

const MODULE_NAME: &str = "tm-tourney-manager";
const HOST: &str = "localhost:1324";

#[component]
pub fn Home() -> impl IntoView {
    let connection: LocalResource<Option<Rc<DbConnection>>> =
        LocalResource::new(move || async move {
            let conn_builder = DbConnection::builder()
                .with_module_name(MODULE_NAME)
                .with_uri(HOST)
                // .with_light_mode(true)
                .on_connect(move |ctx, id, token| println!("HALLO"))
                .on_disconnect(move |_, error| {
                    if let Some(err) = error {
                        leptos::logging::error!("Disconnected with error: {:?}", err);
                    }
                });

            match conn_builder.build().await {
                Ok(conn) => {
                    conn.run_background();
                    Some(Rc::new(conn))
                }
                Err(e) => {
                    leptos::logging::error!("Connection failed with error: {:?}", e);
                    None
                }
            }
        });

    view! {
        <Card class="w-25% sm:w-sm">
            <CardHeader>
                <CardTitle>"Test singlestage component"</CardTitle>
                <CardDescription>
                    "Wonderful subtitle"
                </CardDescription>
            </CardHeader>
            <CardContent>
               <p> a card body </p>
            </CardContent>
            <CardFooter class="flex flex-col items-center gap-2">
                <Button button_type="button" class="w-full">
                    "A button"
                </Button>
            </CardFooter>
        </Card>
    }
}
