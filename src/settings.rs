use crate::app::{AppState, SyncStatus};
use crate::i18n::{Lang, Strings};
use crate::storage;
use crate::sync::{DEFAULT_BRANCH, DEFAULT_PATH, SyncConfig};
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

const TOKEN_URL: &str = "https://github.com/settings/personal-access-tokens/new";

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
                    <SyncSettings />
                </div>
            </div>
        </div>
    }
}

type Label = fn(&Strings) -> &'static str;

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
        apply(draft.get_untracked());
        state.start_sync();
    };
    let clear_token = move |_| {
        draft.update(|c| c.token.clear());
        apply(draft.get_untracked());
        state.sync_status.set(SyncStatus::Idle);
    };

    let field = move |label: Label, kind: &'static str, placeholder: &'static str, get: fn(&SyncConfig) -> &String, set: fn(&mut SyncConfig, String)| {
        view! {
            <label class="field">
                <span>{move || label(t())}</span>
                <input
                    type=kind
                    placeholder=placeholder
                    autocomplete="off"
                    spellcheck="false"
                    prop:value=move || draft.with(|c| get(c).clone())
                    on:input=move |ev| draft.update(|c| set(c, event_target_value(&ev)))
                />
            </label>
        }
    };

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
            {field(|t| t.field_token, "password", "github_pat_…", |c| &c.token, |c, v| c.token = v)}
            <p class="muted small">
                {move || t().token_hint} " "
                <a href=TOKEN_URL target="_blank" rel="noopener">
                    {move || t().create_token}
                </a>
            </p>
            {field(|t| t.field_repo, "text", "owner/repo", |c| &c.repo, |c, v| c.repo = v)}
            <div class="grid2">
                {field(|t| t.field_branch, "text", DEFAULT_BRANCH, |c| &c.branch, |c, v| c.branch = v)}
                {field(|t| t.field_path, "text", DEFAULT_PATH, |c| &c.path, |c, v| c.path = v)}
            </div>
            <label class="check">
                <input
                    type="checkbox"
                    prop:checked=move || draft.with(|c| c.auto)
                    on:change=move |ev| draft.update(|c| c.auto = event_target_checked(&ev))
                />
                {move || t().auto_sync}
            </label>
            {move || status().map(|(class, text)| view! { <p class=format!("sync-status {class}")>{text}</p> })}
            <div class="row-actions">
                <button class="danger" on:click=clear_token>
                    {move || t().clear_token}
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
