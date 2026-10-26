use crate::bibtex;
use crate::doi::{self, Token};
use crate::editor::EntryEditor;
use crate::export::{Export, ExportDialog};
use crate::i18n::{Lang, Strings};
use crate::metadata;
use crate::model::{AddOutcome, Entry, EntryType, Library, View};
use crate::settings::{SettingsDialog, Theme};
use crate::storage::{self, new_id, now};
use crate::sync::{self, SyncConfig, SyncError};
use crate::table::{EntryTable, SortColumn};
use leptos::ev::{DragEvent, KeyboardEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Error,
}

#[derive(Clone, PartialEq)]
pub struct Notice {
    id: u64,
    level: Level,
    text: String,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub lib: RwSignal<Library>,
    pub view: RwSignal<View>,
    pub selected: RwSignal<HashSet<String>>,
    pub editing: RwSignal<Option<String>>,
    pub exporting: RwSignal<Option<Export>>,
    pub notices: RwSignal<Vec<Notice>>,
    pub search: RwSignal<String>,
    /// Column and ascending flag.
    pub sort: RwSignal<(SortColumn, bool)>,
    pub dragging: RwSignal<Vec<String>>,
    /// DOIs waiting for lookup with their target collection.
    pub queue: RwSignal<VecDeque<(String, Option<String>)>>,
    pub busy: RwSignal<bool>,
    pub lang: RwSignal<Lang>,
    pub theme: RwSignal<Theme>,
    pub settings_open: RwSignal<bool>,
    pub sync_config: RwSignal<SyncConfig>,
    pub sync_status: RwSignal<SyncStatus>,
    /// Library as last stored remotely.
    pub synced: RwSignal<Option<Library>>,
    sync_again: RwSignal<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SyncStatus {
    Idle,
    Syncing,
    Done(f64),
    Failed(SyncError),
}

impl AppState {
    pub fn sync_configured(&self) -> bool {
        self.sync_config.with(SyncConfig::is_configured)
    }

    pub fn has_unsynced_changes(&self) -> bool {
        self.sync_configured() && self.synced.with(|s| self.lib.with(|l| s.as_ref() != Some(l)))
    }

    pub fn start_sync(self) {
        if self.sync_status.get_untracked() == SyncStatus::Syncing {
            self.sync_again.set(true);
            return;
        }
        let config = self.sync_config.get_untracked();
        if !config.is_configured() {
            return;
        }
        self.sync_status.set(SyncStatus::Syncing);
        spawn_local(async move {
            let snapshot = self.lib.get_untracked();
            match sync::sync(&config, &snapshot).await {
                Ok(merged) => {
                    let current = self.lib.get_untracked();
                    let next = sync::merge(&current, &merged);
                    self.synced.set(Some(merged));
                    if next != current {
                        self.lib.set(next);
                    }
                    self.sync_status.set(SyncStatus::Done(now()));
                }
                Err(e) => {
                    self.notify(Level::Error, (self.t_untracked().sync_error)(&e));
                    self.sync_status.set(SyncStatus::Failed(e));
                }
            }
            if self.sync_again.get_untracked() {
                self.sync_again.set(false);
                self.start_sync();
            }
        });
    }

    pub fn t(&self) -> &'static Strings {
        self.lang.get().strings()
    }

    /// For event handlers and async tasks.
    pub fn t_untracked(&self) -> &'static Strings {
        self.lang.get_untracked().strings()
    }

    pub fn notify(&self, level: Level, text: impl Into<String>) {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        self.notices.update(|n| {
            n.push(Notice {
                id,
                level,
                text: text.into(),
            });
            if n.len() > 4 {
                n.remove(0);
            }
        });
        let notices = self.notices;
        set_timeout(
            move || notices.update(|n| n.retain(|x| x.id != id)),
            Duration::from_secs(6),
        );
    }

    pub fn current_collection(&self) -> Option<String> {
        match self.view.get_untracked() {
            View::Collection(id) => Some(id),
            View::Library => None,
        }
    }

    pub fn view_name(&self) -> String {
        match self.view.get() {
            View::Library => self.t().library.into(),
            View::Collection(id) => self
                .lib
                .with(|l| l.collection(&id).map(|c| c.name.clone()).unwrap_or_default()),
        }
    }

    /// Selected ids, or every entry of the current view when nothing is selected.
    pub fn export_targets(&self) -> Export {
        let selected: Vec<String> = self.selected.get_untracked().into_iter().collect();
        if !selected.is_empty() {
            return Export {
                ids: selected,
                name: "references".into(),
            };
        }
        let view = self.view.get_untracked();
        let ids = self
            .lib
            .with_untracked(|l| l.entries_in(&view).iter().map(|e| e.id.clone()).collect());
        let name = match view {
            View::Library => "library".into(),
            View::Collection(_) => self.view_name(),
        };
        Export { ids, name }
    }
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState {
        lib: RwSignal::new(storage::load(storage::LIBRARY_KEY)),
        view: RwSignal::new(View::Library),
        selected: RwSignal::new(HashSet::new()),
        editing: RwSignal::new(None),
        exporting: RwSignal::new(None),
        notices: RwSignal::new(vec![]),
        search: RwSignal::new(String::new()),
        sort: RwSignal::new((SortColumn::Added, false)),
        dragging: RwSignal::new(vec![]),
        queue: RwSignal::new(VecDeque::new()),
        busy: RwSignal::new(false),
        lang: RwSignal::new(initial_lang()),
        theme: RwSignal::new(storage::load(storage::THEME_KEY)),
        settings_open: RwSignal::new(false),
        sync_config: RwSignal::new(storage::load(storage::SYNC_KEY)),
        sync_status: RwSignal::new(SyncStatus::Idle),
        synced: RwSignal::new(None),
        sync_again: RwSignal::new(false),
    };
    provide_context(state);

    let generation = Arc::new(AtomicU64::new(0));
    Effect::new(move |_| {
        let auto = state.sync_config.with(|c| c.auto);
        if auto && state.has_unsynced_changes() {
            let current = generation.fetch_add(1, Ordering::Relaxed) + 1;
            let generation = generation.clone();
            set_timeout(
                move || {
                    if generation.load(Ordering::Relaxed) == current {
                        state.start_sync();
                    }
                },
                Duration::from_secs(2),
            );
        }
    });

    Effect::new(move |_| state.lib.with(|l| storage::save(storage::LIBRARY_KEY, l)));
    Effect::new(move |_| {
        let lang = state.lang.get();
        storage::save(storage::LANG_KEY, &Some(lang));
        if let Some(root) = document().document_element() {
            let _ = root.set_attribute("lang", lang.html_tag());
        }
    });
    Effect::new(move |_| {
        let theme = state.theme.get();
        storage::save(storage::THEME_KEY, &theme);
        if let Some(root) = document().document_element() {
            let _ = match theme.attr() {
                Some(value) => root.set_attribute("data-theme", value),
                None => root.remove_attribute("data-theme"),
            };
        }
    });
    Effect::new(move |_| {
        state.view.track();
        state.selected.set(HashSet::new());
    });

    let editing = Memo::new(move |_| state.editing.get());
    let _esc = window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if ev.key() == "Escape" {
            if state.exporting.get_untracked().is_some() {
                state.exporting.set(None);
            } else if state.settings_open.get_untracked() {
                state.settings_open.set(false);
            } else {
                state.editing.set(None);
            }
        }
    });

    view! {
        <div class="app">
            <Sidebar />
            <main class="main">
                <AddBar />
                <Toolbar />
                <EntryTable />
            </main>
            {move || editing.get().map(|id| view! { <EntryEditor id=id /> })}
            {move || state.exporting.get().map(|export| view! { <ExportDialog export=export /> })}
            <Show when=move || state.settings_open.get()>
                <SettingsDialog />
            </Show>
            <Notices />
        </div>
    }
}

fn initial_lang() -> Lang {
    storage::load::<Option<Lang>>(storage::LANG_KEY)
        .unwrap_or_else(|| Lang::from_browser(&window().navigator().language().unwrap_or_default()))
}

#[component]
fn SyncIndicator() -> impl IntoView {
    let state = expect_context::<AppState>();
    let t = move || state.t();
    let label = move || match state.sync_status.get() {
        SyncStatus::Syncing => t().syncing.to_string(),
        SyncStatus::Failed(_) => t().sync_failed.to_string(),
        _ if state.has_unsynced_changes() => t().unsynced.to_string(),
        SyncStatus::Done(time) => (t().synced_at)(&format_time(time)),
        SyncStatus::Idle => t().sync_now.to_string(),
    };
    let class = move || {
        let kind = match state.sync_status.get() {
            SyncStatus::Syncing => "syncing",
            SyncStatus::Failed(_) => "error",
            _ if state.has_unsynced_changes() => "dirty",
            SyncStatus::Done(_) => "ok",
            SyncStatus::Idle => "",
        };
        format!("nav-item footer-button sync {kind}")
    };
    let detail = move || match state.sync_status.get() {
        SyncStatus::Failed(e) => (t().sync_error)(&e),
        _ => t().sync_now.to_string(),
    };
    view! {
        <Show when=move || state.sync_configured()>
            <button class=class title=detail on:click=move |_| state.start_sync()>
                <span class="dot"></span>
                <span class="name">{label}</span>
            </button>
        </Show>
    }
}

fn format_time(ms: f64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}

#[component]
fn SidebarFooter() -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        <div class="sidebar-footer">
            <SyncIndicator />
            <button class="nav-item footer-button" on:click=move |_| state.settings_open.set(true)>
                <IconSettings />
                <span class="name">{move || state.t().settings}</span>
            </button>
        </div>
    }
}

#[component]
fn Sidebar() -> impl IntoView {
    let state = expect_context::<AppState>();
    let creating = RwSignal::new(false);
    let renaming = RwSignal::new(None::<String>);

    let total = move || state.lib.with(|l| l.entries.len());
    let collections = move || {
        state.lib.with(|l| {
            l.collections
                .iter()
                .map(|c| {
                    (
                        c.id.clone(),
                        c.name.clone(),
                        c.entries.iter().filter(|id| l.entry(id).is_some()).count(),
                    )
                })
                .collect::<Vec<_>>()
        })
    };
    let create = move |name: Option<String>| {
        creating.set(false);
        if let Some(name) = name {
            let id = new_id();
            state.lib.update(|l| l.create_collection(id.clone(), &name, now()));
            state.view.set(View::Collection(id));
        }
    };

    view! {
        <nav class="sidebar">
            <div class="brand">
                <span class="logo">"{ }"</span>
                "easy-ref"
            </div>
            <div
                class="nav-item"
                class:active=move || state.view.get() == View::Library
                on:click=move |_| state.view.set(View::Library)
            >
                <IconLibrary />
                <span class="name">{move || state.t().library}</span>
                <span class="count">{total}</span>
            </div>
            <div class="section-title">
                <span>{move || state.t().collections}</span>
                <button class="icon" title=move || state.t().new_collection on:click=move |_| creating.set(true)>
                    "+"
                </button>
            </div>
            {move || {
                collections()
                    .into_iter()
                    .map(|(id, name, count)| view! { <CollectionItem id=id name=name count=count renaming=renaming /> })
                    .collect_view()
            }}
            <Show when=move || creating.get()>
                <NameInput initial=String::new() on_done=create />
            </Show>
            <Show when=move || state.lib.with(|l| l.collections.is_empty()) && !creating.get()>
                <p class="hint">{move || state.t().collections_hint}</p>
            </Show>
            <SidebarFooter />
        </nav>
    }
}

#[component]
fn CollectionItem(id: String, name: String, count: usize, renaming: RwSignal<Option<String>>) -> impl IntoView {
    let state = expect_context::<AppState>();
    let hover = RwSignal::new(false);
    let id = StoredValue::new(id);
    let is_active = move || {
        state
            .view
            .with(|v| matches!(v, View::Collection(c) if *c == id.get_value()))
    };
    let is_renaming = move || renaming.with(|r| r.as_deref() == Some(id.get_value().as_str()));

    let on_drop = move |ev: DragEvent| {
        ev.prevent_default();
        hover.set(false);
        let ids = state.dragging.get_untracked();
        if ids.is_empty() {
            return;
        }
        let mut added = 0;
        state
            .lib
            .update(|l| added = l.add_to_collection(&id.get_value(), &ids, now()));
        state.notify(
            Level::Ok,
            (state.t_untracked().added_to_collection)(added, &name_for(state, &id.get_value())),
        );
    };
    let rename = move |new_name: Option<String>| {
        renaming.set(None);
        if let Some(n) = new_name {
            state.lib.update(|l| l.rename_collection(&id.get_value(), &n, now()));
        }
    };
    let remove = move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        let msg = (state.t_untracked().confirm_delete_collection)(&name_for(state, &id.get_value()));
        if window().confirm_with_message(&msg).unwrap_or(false) {
            state.lib.update(|l| l.delete_collection(&id.get_value(), now()));
            if is_active() {
                state.view.set(View::Library);
            }
        }
    };
    let export = move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        let ids = state.lib.with_untracked(|l| {
            l.entries_in(&View::Collection(id.get_value()))
                .iter()
                .map(|e| e.id.clone())
                .collect()
        });
        state.exporting.set(Some(Export {
            ids,
            name: name_for(state, &id.get_value()),
        }));
    };

    view! {
        <Show
            when=is_renaming
            fallback=move || {
                let name = name.clone();
                view! {
                    <div
                        class="nav-item collection"
                        class:active=is_active
                        class:drophover=move || hover.get()
                        on:click=move |_| state.view.set(View::Collection(id.get_value()))
                        on:dblclick=move |_| renaming.set(Some(id.get_value()))
                        on:dragover=move |ev: DragEvent| {
                            ev.prevent_default();
                            hover.set(true);
                        }
                        on:dragleave=move |_| hover.set(false)
                        on:drop=on_drop
                    >
                        <IconFolder />
                        <span class="name" title=move || state.t().rename_hint>
                            {name}
                        </span>
                        <span class="actions">
                            <button
                                class="icon"
                                title=move || state.t().rename
                                on:click=move |ev: leptos::ev::MouseEvent| {
                                    ev.stop_propagation();
                                    renaming.set(Some(id.get_value()));
                                }
                            >
                                "✎"
                            </button>
                            <button class="icon" title=move || state.t().export_bibtex on:click=export>
                                "⇩"
                            </button>
                            <button class="icon" title=move || state.t().delete_collection on:click=remove>
                                "×"
                            </button>
                        </span>
                        <span class="count">{count}</span>
                    </div>
                }
            }
        >
            <NameInput initial=name_for(state, &id.get_value()) on_done=rename />
        </Show>
    }
}

#[component]
fn IconLibrary() -> impl IntoView {
    view! {
        <svg class="ico" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" />
        </svg>
    }
}

#[component]
fn IconFolder() -> impl IntoView {
    view! {
        <svg class="ico" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
        </svg>
    }
}

#[component]
fn IconSettings() -> impl IntoView {
    view! {
        <svg class="ico" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
            <circle cx="12" cy="12" r="3" />
        </svg>
    }
}

fn name_for(state: AppState, id: &str) -> String {
    state
        .lib
        .with_untracked(|l| l.collection(id).map(|c| c.name.clone()).unwrap_or_default())
}

#[component]
fn NameInput(initial: String, #[prop(into)] on_done: Callback<Option<String>>) -> impl IntoView {
    let state = expect_context::<AppState>();
    let input = NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(el) = input.get() {
            let _ = el.focus();
            el.select();
        }
    });
    let done = Arc::new(AtomicBool::new(false));
    let finish = move |value: Option<String>| {
        if !done.swap(true, Ordering::Relaxed) {
            on_done.run(value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()));
        }
    };
    let finish2 = finish.clone();
    view! {
        <input
            class="name-input"
            node_ref=input
            value=initial
            placeholder=move || state.t().collection_name
            on:keydown=move |ev: KeyboardEvent| match ev.key().as_str() {
                "Enter" => finish(Some(event_target_value(&ev))),
                "Escape" => finish(None),
                _ => {}
            }
            on:blur=move |ev| finish2(Some(event_target_value(&ev)))
        />
    }
}

#[component]
fn AddBar() -> impl IntoView {
    let state = expect_context::<AppState>();
    let text = RwSignal::new(String::new());
    let submit = move || {
        let input = text.get_untracked();
        if !input.trim().is_empty() {
            text.set(String::new());
            add_identifiers(state, &input);
        }
    };
    view! {
        <div class="addbar">
            <textarea
                rows=1
                placeholder=move || state.t().add_placeholder
                prop:value=move || text.get()
                on:input=move |ev| text.set(event_target_value(&ev))
                on:keydown=move |ev: KeyboardEvent| {
                    if ev.key() == "Enter" && !ev.shift_key() {
                        ev.prevent_default();
                        submit();
                    }
                }
            ></textarea>
            <button class="primary" on:click=move |_| submit()>
                {move || state.t().add}
            </button>
            {move || {
                let n = state.queue.with(|q| q.len()) + state.busy.get() as usize;
                (n > 0).then(|| view! { <span class="pending">{(state.t().looking_up)(n)}</span> })
            }}
        </div>
    }
}

fn add_identifiers(state: AppState, input: &str) {
    let target = state.current_collection();
    for token in doi::parse_input(input) {
        let doi = match token {
            Token::Doi(doi) => doi,
            Token::Unrecognized(raw) => {
                state.notify(Level::Warn, (state.t_untracked().unrecognized)(&raw));
                continue;
            }
        };
        if let Some(existing) = state.lib.with_untracked(|l| l.find_by_doi(&doi).map(|e| e.id.clone())) {
            report_duplicate(state, &existing, target.as_deref());
            continue;
        }
        let queued = state
            .queue
            .with_untracked(|q| q.iter().any(|(d, _)| d.eq_ignore_ascii_case(&doi)));
        if !queued {
            state.queue.update(|q| q.push_back((doi, target.clone())));
        }
    }
    if !state.busy.get_untracked() && state.queue.with_untracked(|q| !q.is_empty()) {
        state.busy.set(true);
        spawn_local(run_queue(state));
    }
}

/// Looks DOIs up one at a time; Crossref's public API allows a single concurrent request.
async fn run_queue(state: AppState) {
    let mut added = 0;
    while let Some((doi, target)) = state.queue.try_update(|q| q.pop_front()).flatten() {
        let mut entry = match metadata::lookup(&doi).await {
            Ok(entry) => entry,
            Err(e) => {
                state.notify(Level::Error, (state.t_untracked().lookup_failed)(&doi, &e));
                continue;
            }
        };
        entry.id = new_id();
        entry.key = bibtex::cite_key(&entry);
        let mut outcome = None;
        state.lib.update(|l| outcome = Some(l.add_entry(entry, now())));
        match outcome {
            Some(AddOutcome::Added(id)) => {
                if let Some(cid) = &target {
                    state.lib.update(|l| {
                        l.add_to_collection(cid, std::slice::from_ref(&id), now());
                    });
                }
                added += 1;
            }
            Some(AddOutcome::Duplicate(id)) => report_duplicate(state, &id, target.as_deref()),
            None => {}
        }
    }
    state.busy.set(false);
    if added > 0 {
        state.notify(Level::Ok, (state.t_untracked().added_n)(added));
    }
}

fn report_duplicate(state: AppState, id: &str, target: Option<&str>) {
    let key = state
        .lib
        .with_untracked(|l| l.entry(id).map(|e| e.key.clone()).unwrap_or_default());
    let t = state.t_untracked();
    let Some(cid) = target else {
        return state.notify(Level::Warn, (t.dup_in_library)(&key));
    };
    let mut added = 0;
    state
        .lib
        .update(|l| added = l.add_to_collection(cid, &[id.to_string()], now()));
    if added > 0 {
        state.notify(Level::Ok, (t.dup_added_to_collection)(&key));
    } else {
        state.notify(Level::Warn, (t.dup_in_collection)(&key));
    }
}

#[component]
fn Toolbar() -> impl IntoView {
    let state = expect_context::<AppState>();
    let count = move || state.selected.with(|s| s.len());
    let selection = move || state.selected.get_untracked().into_iter().collect::<Vec<_>>();
    let in_collection = move || matches!(state.view.get(), View::Collection(_));

    let add_to = move |ev: leptos::ev::Event| {
        let select = event_target::<web_sys::HtmlSelectElement>(&ev);
        let cid = select.value();
        select.set_value("");
        if cid.is_empty() {
            return;
        }
        let ids = selection();
        let mut added = 0;
        state.lib.update(|l| added = l.add_to_collection(&cid, &ids, now()));
        state.notify(
            Level::Ok,
            (state.t_untracked().added_to_collection)(added, &name_for(state, &cid)),
        );
    };
    let remove_from = move |_| {
        if let Some(cid) = state.current_collection() {
            let ids = selection();
            state.lib.update(|l| l.remove_from_collection(&cid, &ids, now()));
            state.selected.set(HashSet::new());
        }
    };
    let delete = move |_| {
        let ids = selection();
        let msg = (state.t_untracked().confirm_delete_entries)(ids.len());
        if window().confirm_with_message(&msg).unwrap_or(false) {
            state.lib.update(|l| l.delete_entries(&ids, now()));
            state.selected.set(HashSet::new());
            if state.editing.get_untracked().is_some_and(|e| ids.contains(&e)) {
                state.editing.set(None);
            }
        }
    };
    let new_entry = move |_| {
        let entry = Entry {
            id: new_id(),
            key: "New".into(),
            kind: EntryType::Article,
            ..Default::default()
        };
        let id = entry.id.clone();
        state.lib.update(|l| {
            l.add_entry(entry, now());
            if let Some(cid) = state.current_collection() {
                l.add_to_collection(&cid, std::slice::from_ref(&id), now());
            }
        });
        state.editing.set(Some(id));
    };
    let export = move |_| state.exporting.set(Some(state.export_targets()));

    view! {
        <div class="toolbar">
            <h1>{move || state.view_name()}</h1>
            <span class="muted">
                {move || {
                    let view = state.view.get();
                    (state.t().n_entries)(state.lib.with(|l| l.entries_in(&view).len()))
                }}
            </span>
            <input
                class="search"
                type="search"
                placeholder=move || state.t().search_placeholder
                prop:value=move || state.search.get()
                on:input=move |ev| state.search.set(event_target_value(&ev))
            />
            <div class="spacer"></div>
            <Show when=move || { count() > 0 }>
                <span class="selcount">{move || (state.t().n_selected)(count())}</span>
                <select on:change=add_to>
                    <option value="">{move || state.t().add_to_collection}</option>
                    {move || {
                        state
                            .lib
                            .with(|l| {
                                l.collections
                                    .iter()
                                    .map(|c| view! { <option value=c.id.clone()>{c.name.clone()}</option> })
                                    .collect_view()
                            })
                    }}
                </select>
                <Show when=in_collection>
                    <button on:click=remove_from>{move || state.t().remove_from_collection}</button>
                </Show>
                <button class="danger" on:click=delete>
                    {move || state.t().delete}
                </button>
            </Show>
            <button on:click=new_entry>{move || state.t().new_entry}</button>
            <button class="primary" on:click=export>
                {move || if count() > 0 { state.t().export_selected } else { state.t().export_bibtex }}
            </button>
        </div>
    }
}

#[component]
fn Notices() -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        <div class="notices">
            <For
                each=move || state.notices.get()
                key=|n| n.id
                children=move |n| {
                    let class = match n.level {
                        Level::Ok => "notice ok",
                        Level::Warn => "notice warn",
                        Level::Error => "notice error",
                    };
                    let id = n.id;
                    view! {
                        <div class=class on:click=move |_| state.notices.update(|v| v.retain(|x| x.id != id))>
                            {n.text}
                        </div>
                    }
                }
            />
        </div>
    }
}
