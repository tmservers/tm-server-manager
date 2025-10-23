use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{components::*, path};

use singlestage::*;

mod generated;

mod components;
mod pages;

use crate::pages::home::Home;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Html attr:lang="en" attr:dir="ltr" />

        <Title text="Tournament Manager Admin" />

        <Meta charset="UTF-8" />
        <Meta name="viewport" content="width=device-width, initial-scale=1.0" />

        <ThemeProvider theme=Theme::Yellow>
            <Router>
                <Routes fallback=|| view! { NotFound }>
                    <Route path=path!("/") view=Home />
                </Routes>
            </Router>
        </ThemeProvider>
    }
}
