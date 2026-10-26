mod app;
mod bibtex;
mod doi;
mod editor;
mod export;
mod i18n;
mod metadata;
mod model;
mod settings;
mod storage;
mod sync;
mod table;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
