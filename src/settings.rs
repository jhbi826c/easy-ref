use crate::app::AppState;
use crate::i18n::Lang;
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    /// Value of `data-theme` on `<html>`; none follows the system.
    pub fn attr(self) -> Option<&'static str> {
        match self {
            Theme::System => None,
            Theme::Light => Some("light"),
            Theme::Dark => Some("dark"),
        }
    }
}

#[component]
pub fn SettingsDialog() -> impl IntoView {
    let state = expect_context::<AppState>();
    let t = move || state.t();
    let close = move |_| state.settings_open.set(false);

    view! {
        <div class="backdrop" on:click=close>
            <div class="modal settings" on:click=|ev: MouseEvent| ev.stop_propagation()>
                <header>
                    <h2>{move || t().settings}</h2>
                    <div class="spacer"></div>
                    <button class="icon" on:click=close>
                        "×"
                    </button>
                </header>
                <div class="modal-body">
                    <section>
                        <h3>{move || t().appearance}</h3>
                        <div class="segmented">
                            {Theme::ALL
                                .into_iter()
                                .map(|theme| {
                                    view! {
                                        <button
                                            class:active=move || state.theme.get() == theme
                                            on:click=move |_| state.theme.set(theme)
                                        >
                                            {move || (t().theme_name)(theme)}
                                        </button>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </section>
                    <section>
                        <h3>{move || t().language}</h3>
                        <div class="segmented">
                            {Lang::ALL
                                .into_iter()
                                .map(|lang| {
                                    view! {
                                        <button
                                            class:active=move || state.lang.get() == lang
                                            on:click=move |_| state.lang.set(lang)
                                        >
                                            {lang.name()}
                                        </button>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </section>
                </div>
            </div>
        </div>
    }
}
