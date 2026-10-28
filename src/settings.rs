use crate::app::{AppState, SyncStatus};
use crate::i18n::Lang;
use crate::storage;
use crate::sync::{DEFAULT_REPO, SyncConfig};
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{JsCast, JsValue};

const TOKEN_URL: &str = "https://github.com/settings/tokens/new?scopes=repo&description=easy-ref";
const FINE_GRAINED_URL: &str = "https://github.com/settings/personal-access-tokens/new";

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

/// Title bar colors; same values as in index.html.
const BAR_LIGHT: &str = "#f6f7f9";
const BAR_DARK: &str = "#1d2027";

/// Applies the theme to the page and to the browser or installed app title bar.
pub fn apply_theme(theme: Theme) {
    let doc = document();
    if let Some(root) = doc.document_element() {
        let _ = match theme.attr() {
            Some(value) => root.set_attribute("data-theme", value),
            None => root.remove_attribute("data-theme"),
        };
    }
    let Ok(metas) = doc.query_selector_all("meta[name=theme-color]") else {
        return;
    };
    for meta in (0..metas.length()).filter_map(|i| metas.item(i)?.dyn_into::<web_sys::Element>().ok()) {
        let for_dark = meta.get_attribute("media").is_some_and(|m| m.contains("dark"));
        let color = match theme {
            Theme::Light => BAR_LIGHT,
            Theme::Dark => BAR_DARK,
            Theme::System if for_dark => BAR_DARK,
            Theme::System => BAR_LIGHT,
        };
        let _ = meta.set_attribute("content", color);
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
                    <InstallSettings />
                    <SyncSettings />
                </div>
            </div>
        </div>
    }
}

/// The deferred `beforeinstallprompt` event stored by index.html.
const INSTALL_PROMPT: &str = "easyRefInstall";

fn install_prompt() -> Option<JsValue> {
    js_sys::Reflect::get(&window(), &INSTALL_PROMPT.into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
}

fn is_installed() -> bool {
    window()
        .match_media("(display-mode: standalone)")
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}

#[component]
fn InstallSettings() -> impl IntoView {
    let state = expect_context::<AppState>();
    let t = move || state.t();
    let can_prompt = RwSignal::new(install_prompt().is_some());
    let installed = RwSignal::new(is_installed());

    let ready = window_event_listener_untyped("easy-ref-installable", move |_| can_prompt.set(true));
    let done = window_event_listener_untyped("appinstalled", move |_| {
        installed.set(true);
        can_prompt.set(false);
    });
    on_cleanup(move || {
        ready.remove();
        done.remove();
    });

    let install = move |_| {
        let Some(event) = install_prompt() else { return };
        if let Ok(prompt) =
            js_sys::Reflect::get(&event, &"prompt".into()).and_then(|f| f.dyn_into::<js_sys::Function>())
        {
            let _ = prompt.call0(&event);
        }
        let _ = js_sys::Reflect::set(&window(), &INSTALL_PROMPT.into(), &JsValue::UNDEFINED);
        can_prompt.set(false);
    };

    view! {
        <section>
            <h3>{move || t().install_section}</h3>
            <Show
                when=move || !installed.get()
                fallback=move || view! { <p class="muted">{move || t().installed}</p> }
            >
                <Show when=move || can_prompt.get()>
                    <button class="primary" on:click=install>
                        {move || t().install_button}
                    </button>
                </Show>
                <p class="muted small">{move || t().install_hint}</p>
            </Show>
        </section>
    }
}

#[component]
fn SyncSettings() -> impl IntoView {
    let state = expect_context::<AppState>();
    let t = move || state.t();
    let draft = RwSignal::new(state.sync_config.get_untracked());

    let apply = move |config: SyncConfig| {
        storage::save(storage::SYNC_KEY, &config);
        state.sync_config.set(config);
        state.synced.set(None);
    };
    let save_and_sync = move |_| {
        let d = draft.get_untracked();
        apply(SyncConfig {
            token: d.token.trim().to_string(),
            repo: d.repo.trim().trim_matches('/').to_string(),
            auto: d.auto,
        });
        state.start_sync();
    };
    let disconnect = move |_| {
        draft.update(|c| c.token.clear());
        apply(SyncConfig {
            token: String::new(),
            ..draft.get_untracked()
        });
        state.sync_status.set(SyncStatus::Idle);
    };
    let repo_url = move || {
        state
            .sync_config
            .with(|c| c.repo.contains('/').then(|| format!("https://github.com/{}", c.repo)))
    };
    // The repository found on the first sync.
    Effect::new(move |_| {
        let repo = state.sync_config.with(|c| c.repo.clone());
        draft.update(|d| d.repo = repo);
    });

    let status = move || match state.sync_status.get() {
        SyncStatus::Failed(e) => Some(("error", (t().sync_error)(&e))),
        SyncStatus::Syncing => Some(("", t().syncing.to_string())),
        SyncStatus::Done(_) => Some(("ok", t().sync_done.to_string())),
        SyncStatus::Idle => None,
    };

    view! {
        <section class="sync-settings">
            <h3>{move || t().sync_section}</h3>
            <p class="muted">{move || t().sync_intro}</p>
            <label class="field">
                <span>{move || t().field_token}</span>
                <input
                    type="password"
                    placeholder="ghp_…"
                    autocomplete="off"
                    spellcheck="false"
                    prop:value=move || draft.with(|c| c.token.clone())
                    on:input=move |ev| draft.update(|c| c.token = event_target_value(&ev))
                />
            </label>
            <p class="muted small">
                {move || t().token_hint} " "
                <a href=TOKEN_URL target="_blank" rel="noopener">
                    {move || t().create_token}
                </a>
            </p>
            <label class="field">
                <span>{move || t().field_repo}</span>
                <input
                    type="text"
                    placeholder=DEFAULT_REPO
                    autocomplete="off"
                    spellcheck="false"
                    prop:value=move || draft.with(|c| c.repo.clone())
                    on:input=move |ev| draft.update(|c| c.repo = event_target_value(&ev))
                />
            </label>
            <p class="muted small">
                {move || t().repo_hint} " "
                <a href=FINE_GRAINED_URL target="_blank" rel="noopener">
                    {move || t().create_fine_grained}
                </a>
            </p>
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=move || draft.with(|c| c.auto)
                    on:change=move |ev| draft.update(|c| c.auto = event_target_checked(&ev))
                />
                {move || t().auto_sync}
            </label>
            {move || status().map(|(class, text)| view! { <p class=format!("sync-status {class}")>{text}</p> })}
            {move || {
                repo_url()
                    .map(|url| {
                        view! {
                            <p class="small">
                                <a href=url target="_blank" rel="noopener">
                                    {move || t().open_repo}
                                </a>
                            </p>
                        }
                    })
            }}
            <div class="row-actions">
                <button class="danger" on:click=disconnect>
                    {move || t().disconnect}
                </button>
                <div class="spacer"></div>
                <button
                    class="primary"
                    disabled=move || !draft.with(SyncConfig::is_configured)
                    on:click=save_and_sync
                >
                    {move || t().save_and_sync}
                </button>
            </div>
        </section>
    }
}
