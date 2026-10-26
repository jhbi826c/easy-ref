mod bibtex;
mod doi;
mod metadata;
mod model;
mod storage;

use leptos::prelude::*;

#[component]
fn App() -> impl IntoView {
    view! { <h1>"easy-ref"</h1> }
}

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
