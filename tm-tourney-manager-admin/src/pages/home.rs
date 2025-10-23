use leptos::prelude::*;
use singlestage::{Button, card::*};

#[component]
pub fn Home() -> impl IntoView {
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
