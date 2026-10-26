use crate::bibtex;
use crate::doi::{self, Token};
use crate::editor::EntryEditor;
use crate::export::{Export, ExportDialog};
use crate::i18n::{Lang, Strings};
use crate::metadata;
use crate::model::{AddOutcome, Entry, EntryType, Library, View};
use crate::storage::{self, new_id, now};
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
}

impl AppState {
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
            n.push(Notice { id, level, text: text.into() });
            if n.len() > 4 {
                n.remove(0);
            }
        });
        let notices = self.notices;
        set_timeout(move || notices.update(|n| n.retain(|x| x.id != id)), Duration::from_secs(6));
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
            View::Collection(id) => self.lib.with(|l| l.collection(&id).map(|c| c.name.clone()).unwrap_or_default()),
        }
    }

    /// Selected ids, or every entry of the current view when nothing is selected.
    pub fn export_targets(&self) -> Export {
        let selected: Vec<String> = self.selected.get_untracked().into_iter().collect();
        if !selected.is_empty() {
            return Export { ids: selected, name: "references".into() };
        }
        let view = self.view.get_untracked();
        let ids = self.lib.with_untracked(|l| l.entries_in(&view).iter().map(|e| e.id.clone()).collect());
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
    };
    provide_context(state);

    Effect::new(move |_| state.lib.with(|l| storage::save(storage::LIBRARY_KEY, l)));
    Effect::new(move |_| {
        let lang = state.lang.get();
        storage::save(storage::LANG_KEY, &Some(lang));
        if let Some(root) = document().document_element() {
            let _ = root.set_attribute("lang", lang.html_tag());
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
            <Notices />
        </div>
    }
}

fn initial_lang() -> Lang {
    storage::load::<Option<Lang>>(storage::LANG_KEY)
        .unwrap_or_else(|| Lang::from_browser(&window().navigator().language().unwrap_or_default()))
}

#[component]
fn LangSwitch() -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        <div class="lang-switch">
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
                .map(|c| (c.id.clone(), c.name.clone(), c.entries.iter().filter(|id| l.entry(id).is_some()).count()))
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
            <div class="sidebar-footer">
                <LangSwitch />
            </div>
        </nav>
    }
}

#[component]
fn CollectionItem(id: String, name: String, count: usize, renaming: RwSignal<Option<String>>) -> impl IntoView {
    let state = expect_context::<AppState>();
    let hover = RwSignal::new(false);
    let id = StoredValue::new(id);
    let is_active = move || state.view.with(|v| matches!(v, View::Collection(c) if *c == id.get_value()));
    let is_renaming = move || renaming.with(|r| r.as_deref() == Some(id.get_value().as_str()));

    let on_drop = move |ev: DragEvent| {
        ev.prevent_default();
        hover.set(false);
        let ids = state.dragging.get_untracked();
        if ids.is_empty() {
            return;
        }
        let mut added = 0;
        state.lib.update(|l| added = l.add_to_collection(&id.get_value(), &ids, now()));
        state.notify(Level::Ok, (state.t_untracked().added_to_collection)(added, &name_for(state, &id.get_value())));
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
            l.entries_in(&View::Collection(id.get_value())).iter().map(|e| e.id.clone()).collect()
        });
        state.exporting.set(Some(Export { ids, name: name_for(state, &id.get_value()) }));
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
        <svg class="ico" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 19.5v-15A2.5 2.5 0 0 1 6.5 2H20v20H6.5a2.5 2.5 0 0 1 0-5H20" />
        </svg>
    }
}

#[component]
fn IconFolder() -> impl IntoView {
    view! {
        <svg class="ico" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
        </svg>
    }
}

fn name_for(state: AppState, id: &str) -> String {
    state.lib.with_untracked(|l| l.collection(id).map(|c| c.name.clone()).unwrap_or_default())
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
        let queued = state.queue.with_untracked(|q| q.iter().any(|(d, _)| d.eq_ignore_ascii_case(&doi)));
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
    let key = state.lib.with_untracked(|l| l.entry(id).map(|e| e.key.clone()).unwrap_or_default());
    let t = state.t_untracked();
    let Some(cid) = target else {
        return state.notify(Level::Warn, (t.dup_in_library)(&key));
    };
    let mut added = 0;
    state.lib.update(|l| added = l.add_to_collection(cid, &[id.to_string()], now()));
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
        state.notify(Level::Ok, (state.t_untracked().added_to_collection)(added, &name_for(state, &cid)));
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
