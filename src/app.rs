use crate::bibparse;
use crate::bibtex;
use crate::doi::{self, Token};
use crate::editor::EntryEditor;
use crate::export::{Export, ExportDialog};
use crate::i18n::{Lang, Strings};
use crate::metadata;
use crate::model::{AddOutcome, Entry, EntryType, Library, Place, View};
use crate::online;
use crate::settings::{self, SettingsDialog, Theme};
use crate::storage::{self, new_id, now};
use crate::sync::{self, SyncConfig, SyncError};
use crate::table::{EntryTable, SortColumn};
use leptos::ev::{DragEvent, KeyboardEvent, PointerEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use wasm_bindgen::JsCast;

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
    pub dragging_collection: RwSignal<Option<String>>,
    /// DOIs waiting for lookup with their target collection.
    pub queue: RwSignal<VecDeque<(String, Option<String>)>>,
    pub busy: RwSignal<bool>,
    /// Entries added or found as duplicates by the current batch of lookups.
    batch: RwSignal<Vec<String>>,
    /// Just-added entries, marked in the table until the selection changes.
    pub recent: RwSignal<HashSet<String>>,
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
                Ok((merged, gist_id)) => {
                    let current = self.lib.get_untracked();
                    let next = sync::merge(&current, &merged);
                    self.synced.set(Some(merged));
                    if next != current {
                        self.lib.set(next);
                    }
                    if self.sync_config.with_untracked(|c| c.gist_id != gist_id) {
                        self.sync_config.update(|c| c.gist_id = gist_id);
                        self.sync_config.with_untracked(|c| storage::save(storage::SYNC_KEY, c));
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
            View::Library | View::Trash => None,
        }
    }

    pub fn view_name(&self) -> String {
        match self.view.get() {
            View::Library => self.t().library.into(),
            View::Trash => self.t().trash.into(),
            View::Collection(id) => self.lib.with(|l| l.collection_path(&id)),
        }
    }

    /// Marks just-added entries without touching the selection and scrolls the latest into view.
    pub fn reveal(&self, ids: &[String]) {
        let Some(latest) = ids.last().cloned() else { return };
        self.recent.set(ids.iter().cloned().collect());
        request_animation_frame(move || {
            if let Ok(Some(row)) = document().query_selector(&format!("tr[data-id=\"{latest}\"]")) {
                let options = web_sys::ScrollIntoViewOptions::new();
                options.set_block(web_sys::ScrollLogicalPosition::Nearest);
                row.scroll_into_view_with_scroll_into_view_options(&options);
            }
        });
    }

    /// Adds an entry to the current batch and reveals the whole batch.
    fn mark_in_batch(&self, id: &str) {
        self.batch.update(|b| {
            if !b.iter().any(|x| x == id) {
                b.push(id.to_string());
            }
        });
        self.batch.with_untracked(|b| self.reveal(b));
    }

    pub fn in_trash(&self) -> bool {
        self.view.with(|v| *v == View::Trash)
    }

    /// Moves entries to the trash, or deletes them for good when they already are there.
    pub fn delete_entries(&self, ids: Vec<String>) {
        let t = self.t_untracked();
        let in_trash = self
            .lib
            .with_untracked(|l| ids.iter().all(|id| l.entry(id).is_some_and(|e| e.trashed.is_some())));
        if in_trash {
            if !window()
                .confirm_with_message(&(t.confirm_delete_entries)(ids.len()))
                .unwrap_or(false)
            {
                return;
            }
            self.lib.update(|l| l.delete_entries(&ids, now()));
        } else {
            self.lib.update(|l| l.trash_entries(&ids, now()));
            self.notify(Level::Ok, (t.moved_to_trash)(ids.len()));
        }
        self.selected.set(HashSet::new());
        if self.editing.get_untracked().is_some_and(|e| ids.contains(&e)) {
            self.editing.set(None);
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
            View::Library | View::Trash => "library".into(),
            View::Collection(id) => name_for(*self, &id),
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
        sort: RwSignal::new((SortColumn::Year, false)),
        dragging: RwSignal::new(vec![]),
        dragging_collection: RwSignal::new(None),
        queue: RwSignal::new(VecDeque::new()),
        busy: RwSignal::new(false),
        batch: RwSignal::new(vec![]),
        recent: RwSignal::new(HashSet::new()),
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
        settings::apply_theme(theme);
    });
    Effect::new(move |_| {
        state.view.track();
        state.selected.set(HashSet::new());
    });
    Effect::new(move |_| {
        state.selected.track();
        state.recent.set(HashSet::new());
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

    let sidebar_width = RwSignal::new(storage::load::<Option<f64>>(storage::SIDEBAR_KEY).unwrap_or(SIDEBAR_DEFAULT));

    view! {
        <div class="app" style=move || format!("--sidebar-width: {}px", sidebar_width.get())>
            <Sidebar />
            <SidebarResizer width=sidebar_width />
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

const SIDEBAR_DEFAULT: f64 = 230.0;
const SIDEBAR_MIN: f64 = 160.0;
const SIDEBAR_MAX: f64 = 480.0;

#[component]
fn SidebarResizer(width: RwSignal<f64>) -> impl IntoView {
    let state = expect_context::<AppState>();
    let dragging = RwSignal::new(false);
    let save = move || storage::save(storage::SIDEBAR_KEY, &Some(width.get_untracked()));

    let on_down = move |ev: PointerEvent| {
        ev.prevent_default();
        if let Some(el) = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
            let _ = el.set_pointer_capture(ev.pointer_id());
        }
        dragging.set(true);
    };
    let on_move = move |ev: PointerEvent| {
        if dragging.get_untracked() {
            width.set((ev.client_x() as f64).clamp(SIDEBAR_MIN, SIDEBAR_MAX));
        }
    };
    let on_up = move |_: PointerEvent| {
        if dragging.get_untracked() {
            dragging.set(false);
            save();
        }
    };

    view! {
        <div
            class="resizer"
            class:dragging=move || dragging.get()
            title=move || state.t().resize_sidebar
            on:pointerdown=on_down
            on:pointermove=on_move
            on:pointerup=on_up
            on:pointercancel=on_up
            on:dblclick=move |_| {
                width.set(SIDEBAR_DEFAULT);
                save();
            }
        ></div>
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
            <TrashItem />
            <SyncIndicator />
            <button class="nav-item footer-button" on:click=move |_| state.settings_open.set(true)>
                <IconSettings />
                <span class="name">{move || state.t().settings}</span>
            </button>
        </div>
    }
}

/// A sidebar row: collection id, name, depth, entry count and whether it has children.
type TreeRow = (String, String, usize, usize, bool);

/// Visible rows of the collection tree, skipping children of collapsed collections.
fn visible_rows(lib: &Library, collapsed: &HashSet<String>) -> Vec<TreeRow> {
    let tree = lib.tree();
    let mut rows = Vec::new();
    let mut hidden_below: Option<usize> = None;
    for (i, (c, depth)) in tree.iter().enumerate() {
        if hidden_below.is_some_and(|d| *depth > d) {
            continue;
        }
        hidden_below = None;
        let has_children = tree.get(i + 1).is_some_and(|(_, next)| *next > *depth);
        if has_children && collapsed.contains(&c.id) {
            hidden_below = Some(*depth);
        }
        let count = lib.entries_in(&View::Collection(c.id.clone())).len();
        rows.push((c.id.clone(), c.name.clone(), *depth, count, has_children));
    }
    rows
}

fn indent(depth: usize) -> String {
    format!("padding-left: {}px", 10 + depth * 14)
}

#[component]
fn Sidebar() -> impl IntoView {
    let state = expect_context::<AppState>();
    // None: not creating; Some(None): a top-level collection; Some(Some(id)): a child of `id`.
    let creating = RwSignal::new(None::<Option<String>>);
    let renaming = RwSignal::new(None::<String>);
    let collapsed = RwSignal::new(storage::load::<HashSet<String>>(storage::COLLAPSED_KEY));
    Effect::new(move |_| collapsed.with(|c| storage::save(storage::COLLAPSED_KEY, c)));

    let total = move || state.lib.with(|l| l.entries_in(&View::Library).len());
    let library_hover = RwSignal::new(false);
    let restore_dropped = move |ev: DragEvent| {
        ev.prevent_default();
        library_hover.set(false);
        let ids = state.dragging.get_untracked();
        if !ids.is_empty() {
            state.lib.update(|l| l.restore_entries(&ids, now()));
            state.selected.set(HashSet::new());
            state.notify(Level::Ok, (state.t_untracked().restored)(ids.len()));
        }
    };
    let create = move |name: Option<String>| {
        let parent = creating.get_untracked().flatten();
        creating.set(None);
        if let Some(name) = name {
            let id = new_id();
            state
                .lib
                .update(|l| l.create_collection(id.clone(), &name, parent.as_deref(), now()));
            state.view.set(View::Collection(id));
        }
    };
    let rows = move || {
        let collapsed = collapsed.get();
        state.lib.with(|l| visible_rows(l, &collapsed))
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
                class:drophover=move || library_hover.get()
                on:click=move |_| state.view.set(View::Library)
                on:dragover=move |ev: DragEvent| {
                    if state.in_trash() && !state.dragging.with_untracked(Vec::is_empty) {
                        ev.prevent_default();
                        library_hover.set(true);
                    }
                }
                on:dragleave=move |_| library_hover.set(false)
                on:drop=restore_dropped
            >
                <IconLibrary />
                <span class="name">{move || state.t().library}</span>
                <span class="count">{total}</span>
            </div>
            <div class="section-title">
                <span>{move || state.t().collections}</span>
                <button class="icon" title=move || state.t().new_collection on:click=move |_| creating.set(Some(None))>
                    "+"
                </button>
            </div>
            {move || {
                rows()
                    .into_iter()
                    .map(|(id, name, depth, count, has_children)| {
                        let adding_child = creating.with(|c| c.as_ref().and_then(|p| p.as_deref()) == Some(id.as_str()));
                        view! {
                            <CollectionItem
                                id=id
                                name=name
                                depth=depth
                                count=count
                                has_children=has_children
                                collapsed=collapsed
                                renaming=renaming
                                creating=creating
                            />
                            {adding_child
                                .then(|| {
                                    view! {
                                        <div style=indent(depth + 1)>
                                            <NameInput initial=String::new() on_done=create />
                                        </div>
                                    }
                                })}
                        }
                    })
                    .collect_view()
            }}
            <Show when=move || creating.with(|c| c == &Some(None))>
                <NameInput initial=String::new() on_done=create />
            </Show>
            <Show when=move || state.lib.with(|l| l.collections.is_empty()) && creating.with(Option::is_none)>
                <p class="hint">{move || state.t().collections_hint}</p>
            </Show>
            <SidebarFooter />
        </nav>
    }
}

#[component]
fn CollectionItem(
    id: String,
    name: String,
    depth: usize,
    count: usize,
    has_children: bool,
    collapsed: RwSignal<HashSet<String>>,
    renaming: RwSignal<Option<String>>,
    creating: RwSignal<Option<Option<String>>>,
) -> impl IntoView {
    let state = expect_context::<AppState>();
    let hover = RwSignal::new(false);
    let id = StoredValue::new(id);
    let is_active = move || {
        state
            .view
            .with(|v| matches!(v, View::Collection(c) if *c == id.get_value()))
    };
    let is_renaming = move || renaming.with(|r| r.as_deref() == Some(id.get_value().as_str()));
    let is_open = move || !collapsed.with(|c| c.contains(&id.get_value()));
    let item = NodeRef::<leptos::html::Div>::new();
    let drop_place = RwSignal::new(None::<Place>);
    let is_dragged = move || {
        state
            .dragging_collection
            .with(|d| d.as_deref() == Some(id.get_value().as_str()))
    };

    let on_dragstart = move |ev: DragEvent| {
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &name_for(state, &id.get_value()));
            dt.set_effect_allowed("move");
        }
        state.dragging_collection.set(Some(id.get_value()));
    };
    let on_dragover = move |ev: DragEvent| {
        ev.prevent_default();
        match state.dragging_collection.get_untracked() {
            Some(dragged) if dragged != id.get_value() => {
                let place = item.get_untracked().map(|el| {
                    let rect = el.get_bounding_client_rect();
                    let at = (f64::from(ev.client_y()) - rect.top()) / rect.height();
                    if at < 0.25 {
                        Place::Before
                    } else if at > 0.75 {
                        Place::After
                    } else {
                        Place::Inside
                    }
                });
                drop_place.set(place);
            }
            Some(_) => {}
            None => hover.set(true),
        }
    };
    let on_drop = move |ev: DragEvent| {
        ev.prevent_default();
        hover.set(false);
        if let Some(dragged) = state.dragging_collection.get_untracked() {
            let place = drop_place.get_untracked().unwrap_or(Place::Inside);
            drop_place.set(None);
            state.dragging_collection.set(None);
            if place == Place::Inside {
                collapsed.update(|c| {
                    c.remove(&id.get_value());
                });
            }
            state
                .lib
                .update(|l| l.move_collection(&dragged, &id.get_value(), place, now()));
            return;
        }
        let ids = state.dragging.get_untracked();
        if ids.is_empty() {
            return;
        }
        let mut added = 0;
        state.lib.update(|l| {
            l.restore_entries(&ids, now());
            added = l.add_to_collection(&id.get_value(), &ids, now());
        });
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
        let subfolders = state.lib.with_untracked(|l| l.descendants(&id.get_value()).len());
        let msg = (state.t_untracked().confirm_delete_collection)(&name_for(state, &id.get_value()), subfolders);
        if window().confirm_with_message(&msg).unwrap_or(false) {
            let doomed = state.lib.with_untracked(|l| {
                let mut ids = l.descendants(&id.get_value());
                ids.push(id.get_value());
                ids
            });
            state.lib.update(|l| l.delete_collection(&id.get_value(), now()));
            if state
                .view
                .with_untracked(|v| matches!(v, View::Collection(c) if doomed.contains(c)))
            {
                state.view.set(View::Library);
            }
        }
    };
    let add_child = move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        collapsed.update(|c| {
            c.remove(&id.get_value());
        });
        creating.set(Some(Some(id.get_value())));
    };
    let toggle = move |ev: leptos::ev::MouseEvent| {
        ev.stop_propagation();
        collapsed.update(|c| {
            let id = id.get_value();
            if !c.remove(&id) {
                c.insert(id);
            }
        });
    };

    view! {
        <Show
            when=is_renaming
            fallback=move || {
                let name = name.clone();
                view! {
                    <div
                        class="nav-item collection"
                        style=indent(depth)
                        node_ref=item
                        draggable="true"
                        class:active=is_active
                        class:drophover=move || hover.get()
                        class:dropbefore=move || drop_place.get() == Some(Place::Before)
                        class:dropafter=move || drop_place.get() == Some(Place::After)
                        class:dropinside=move || drop_place.get() == Some(Place::Inside)
                        class:dragged=is_dragged
                        on:click=move |_| state.view.set(View::Collection(id.get_value()))
                        on:dblclick=move |_| renaming.set(Some(id.get_value()))
                        on:dragstart=on_dragstart
                        on:dragend=move |_| {
                            state.dragging_collection.set(None);
                            drop_place.set(None);
                        }
                        on:dragover=on_dragover
                        on:dragleave=move |_| {
                            hover.set(false);
                            drop_place.set(None);
                        }
                        on:drop=on_drop
                    >
                        {if has_children {
                            view! {
                                <button class="icon twisty" class:open=is_open on:click=toggle>
                                    "▸"
                                </button>
                            }
                                .into_any()
                        } else {
                            view! { <span class="twisty"></span> }.into_any()
                        }}
                        <IconFolder />
                        <span class="name" title=move || state.t().rename_hint>
                            {name}
                        </span>
                        <span class="actions">
                            <button class="icon" title=move || state.t().new_subcollection on:click=add_child>
                                "+"
                            </button>
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
                            <button class="icon" title=move || state.t().delete_collection on:click=remove>
                                "×"
                            </button>
                        </span>
                        <span class="count">{count}</span>
                    </div>
                }
            }
        >
            <div style=indent(depth)>
                <NameInput initial=name_for(state, &id.get_value()) on_done=rename />
            </div>
        </Show>
    }
}

#[component]
fn TrashItem() -> impl IntoView {
    let state = expect_context::<AppState>();
    let hover = RwSignal::new(false);
    let count = move || state.lib.with(|l| l.entries_in(&View::Trash).len());
    let on_drop = move |ev: DragEvent| {
        ev.prevent_default();
        hover.set(false);
        let ids = state.dragging.get_untracked();
        if !ids.is_empty() && state.view.get_untracked() != View::Trash {
            state.delete_entries(ids);
        }
    };
    view! {
        <div
            class="nav-item trash"
            class:active=move || state.view.get() == View::Trash
            class:drophover=move || hover.get()
            on:click=move |_| state.view.set(View::Trash)
            on:dragover=move |ev: DragEvent| {
                if state.dragging_collection.get_untracked().is_none() {
                    ev.prevent_default();
                    hover.set(true);
                }
            }
            on:dragleave=move |_| hover.set(false)
            on:drop=on_drop
        >
            <IconTrash />
            <span class="name">{move || state.t().trash}</span>
            <span class="count">{count}</span>
        </div>
    }
}

#[component]
fn IconTrash() -> impl IntoView {
    view! {
        <svg class="ico" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 6h18" />
            <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
            <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
        </svg>
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
    let is_bib = Memo::new(move |_| text.with(|t| bibparse::looks_like_bibtex(t)));
    let area = NodeRef::<leptos::html::Textarea>::new();

    // Fit the height to the content.
    Effect::new(move |_| {
        text.track();
        request_animation_frame(move || {
            if let Some(el) = area.get_untracked() {
                let style = web_sys::HtmlElement::style(&el);
                let _ = style.set_property("height", "auto");
                let _ = style.set_property("height", &format!("{}px", el.scroll_height() + 2));
            }
        });
    });

    let submit = move || {
        let input = text.get_untracked();
        if input.trim().is_empty() {
            return;
        }
        if bibparse::looks_like_bibtex(&input) {
            if import_bibtex(state, &input) {
                text.set(String::new());
            }
        } else {
            text.set(String::new());
            add_identifiers(state, &input);
        }
    };
    view! {
        <div class="addbar">
            <div class="addbar-input">
                <textarea
                    rows=1
                    node_ref=area
                    spellcheck="false"
                    placeholder=move || state.t().add_placeholder
                    prop:value=move || text.get()
                    on:input=move |ev| text.set(event_target_value(&ev))
                    on:keydown=move |ev: KeyboardEvent| {
                        if ev.key() != "Enter" || ev.is_composing() {
                            return;
                        }
                        let force = ev.meta_key() || ev.ctrl_key();
                        if force || (!ev.shift_key() && !is_bib.get_untracked()) {
                            ev.prevent_default();
                            submit();
                        }
                    }
                ></textarea>
                <Show when=move || is_bib.get()>
                    <div class="addbar-hint">{move || state.t().bib_detected}</div>
                </Show>
            </div>
            <button class="primary" on:click=move |_| submit()>
                {move || if is_bib.get() { state.t().import } else { state.t().add }}
            </button>
            {move || {
                let n = state.queue.with(|q| q.len()) + state.busy.get() as usize;
                (n > 0).then(|| view! { <span class="pending">{(state.t().looking_up)(n)}</span> })
            }}
        </div>
    }
}

/// Returns true if every entry in `text` was parsed.
pub fn import_bibtex(state: AppState, text: &str) -> bool {
    let t = state.t_untracked();
    let (raw, errors) = bibparse::parse(text);
    for line in errors.iter().take(3) {
        state.notify(Level::Warn, (t.bib_parse_error)(*line));
    }
    if raw.is_empty() {
        if errors.is_empty() {
            state.notify(Level::Warn, t.no_bib_entries);
        }
        return false;
    }
    let target = state.current_collection();
    let (mut added, mut skipped) = (0, 0);
    let mut replaced = Vec::new();
    let mut new_ids = Vec::new();
    state.lib.update(|l| {
        for r in &raw {
            let mut entry = bibparse::to_entry(r);
            entry.id = new_id();
            entry.key = bibtex::cite_key(&entry);
            let id = match l.add_entry(entry, now()) {
                AddOutcome::Added(id) => {
                    added += 1;
                    new_ids.push(id.clone());
                    id
                }
                AddOutcome::Duplicate(id) => {
                    skipped += 1;
                    new_ids.push(id.clone());
                    id
                }
                AddOutcome::Replaced { id, old_key } => {
                    replaced.push((old_key, l.entry(&id).map(|e| e.key.clone()).unwrap_or_default()));
                    new_ids.push(id.clone());
                    id
                }
            };
            if let Some(cid) = &target {
                l.add_to_collection(cid, &[id], now());
            }
        }
    });
    state.reveal(&new_ids);
    if added > 0 || skipped > 0 {
        state.notify(Level::Ok, (t.imported)(added, skipped));
    }
    for (old_key, new_key) in replaced {
        state.notify(Level::Ok, (t.replaced)(&old_key, &new_key));
    }
    errors.is_empty()
}

fn add_identifiers(state: AppState, input: &str) {
    let target = state.current_collection();
    if !state.busy.get_untracked() {
        state.batch.set(vec![]);
    }
    for token in doi::parse_input(input) {
        let doi = match token {
            Token::Doi(doi) => doi,
            Token::Unrecognized(raw) => {
                match online::host(&raw) {
                    Some(host) if online::is_paper_page(&host) => {
                        state.notify(Level::Warn, (state.t_untracked().paper_page)(&host));
                    }
                    Some(_) => queue_link(state, raw, target.clone()),
                    None => state.notify(Level::Warn, (state.t_untracked().unrecognized)(&raw)),
                }
                continue;
            }
        };
        if let Some(existing) = state.lib.with_untracked(|l| l.find_by_doi(&doi).map(|e| e.id.clone())) {
            report_duplicate(state, &existing, target.as_deref());
            state.mark_in_batch(&existing);
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

/// Queues a web link unless an online entry with the same link exists.
fn queue_link(state: AppState, url: String, target: Option<String>) {
    if let Some(existing) = state.lib.with_untracked(|l| l.find_by_url(&url).map(|e| e.id.clone())) {
        report_duplicate(state, &existing, target.as_deref());
        state.mark_in_batch(&existing);
        return;
    }
    let queued = state.queue.with_untracked(|q| {
        q.iter()
            .any(|(u, _)| online::normalize_url(u) == online::normalize_url(&url))
    });
    if !queued {
        state.queue.update(|q| q.push_back((url, target)));
    }
}

/// An online entry for a web link, with repository details for GitHub links.
async fn link_entry(state: AppState, url: &str) -> Entry {
    let year = js_sys::Date::new_0().get_full_year() as i32;
    let entry = online::link_entry(url, &crate::editor::today(), year);
    let Some((owner, repo)) = online::github_repo(url) else {
        return entry;
    };
    let token = state.sync_config.with_untracked(|c| c.token.trim().to_string());
    match online::lookup_github(&owner, &repo, &token, entry.clone()).await {
        Ok(filled) => filled,
        Err(e) => {
            state.notify(Level::Warn, (state.t_untracked().repo_failed)(url, &e));
            entry
        }
    }
}

/// Looks up queued DOIs and web links one at a time.
async fn run_queue(state: AppState) {
    let mut added = 0;
    while let Some((item, target)) = state.queue.try_update(|q| q.pop_front()).flatten() {
        let mut entry = if online::host(&item).is_some() {
            link_entry(state, &item).await
        } else {
            match metadata::lookup(&item).await {
                Ok(entry) => entry,
                Err(e) => {
                    state.notify(Level::Error, (state.t_untracked().lookup_failed)(&item, &e));
                    continue;
                }
            }
        };
        let needs_title = entry.title.trim().is_empty();
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
                state.mark_in_batch(&id);
                if needs_title {
                    state.editing.set(Some(id));
                }
            }
            Some(AddOutcome::Duplicate(id)) => {
                report_duplicate(state, &id, target.as_deref());
                state.mark_in_batch(&id);
            }
            Some(AddOutcome::Replaced { id, old_key }) => {
                let new_key = state
                    .lib
                    .with_untracked(|l| l.entry(&id).map(|e| e.key.clone()).unwrap_or_default());
                if let Some(cid) = &target {
                    state.lib.update(|l| {
                        l.add_to_collection(cid, std::slice::from_ref(&id), now());
                    });
                }
                state.notify(Level::Ok, (state.t_untracked().replaced)(&old_key, &new_key));
                state.mark_in_batch(&id);
            }
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
    let delete = move |_| state.delete_entries(selection());
    let restore = move |_| {
        let ids = selection();
        state.lib.update(|l| l.restore_entries(&ids, now()));
        state.selected.set(HashSet::new());
        state.notify(Level::Ok, (state.t_untracked().restored)(ids.len()));
    };
    let trash_count = move || state.lib.with(|l| l.entries_in(&View::Trash).len());
    let empty_trash = move |_| {
        let ids: Vec<String> = state
            .lib
            .with_untracked(|l| l.entries_in(&View::Trash).iter().map(|e| e.id.clone()).collect());
        if window()
            .confirm_with_message(&(state.t_untracked().confirm_empty_trash)(ids.len()))
            .unwrap_or(false)
        {
            state.lib.update(|l| l.delete_entries(&ids, now()));
            state.selected.set(HashSet::new());
            state.editing.set(None);
        }
    };
    let file_input = NodeRef::<leptos::html::Input>::new();
    let import_files = move |ev: leptos::ev::Event| {
        let input = event_target::<web_sys::HtmlInputElement>(&ev);
        let Some(files) = input.files() else { return };
        for file in (0..files.length()).filter_map(|i| files.get(i)) {
            let promise = file.text();
            spawn_local(async move {
                if let Some(text) = wasm_bindgen_futures::JsFuture::from(promise)
                    .await
                    .ok()
                    .and_then(|v| v.as_string())
                {
                    let _ = import_bibtex(state, &text);
                }
            });
        }
        input.set_value("");
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
        state.reveal(std::slice::from_ref(&id));
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
            </Show>
            <Show when=move || { count() > 0 && !state.in_trash() }>
                <select on:change=add_to>
                    <option value="">{move || state.t().add_to_collection}</option>
                    {move || {
                        state
                            .lib
                            .with(|l| {
                                l.tree()
                                    .into_iter()
                                    .map(|(c, _)| {
                                        view! { <option value=c.id.clone()>{l.collection_path(&c.id)}</option> }
                                    })
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
            <Show
                when=move || !state.in_trash()
                fallback=move || {
                    view! {
                        <button disabled=move || count() == 0 title=move || state.t().select_to_restore on:click=restore>
                            {move || state.t().restore}
                        </button>
                        <button class="danger" disabled=move || count() == 0 on:click=delete>
                            {move || state.t().delete_forever}
                        </button>
                        <button class="danger" disabled=move || trash_count() == 0 on:click=empty_trash>
                            {move || state.t().empty_trash}
                        </button>
                    }
                }
            >
                <input
                    class="hidden"
                    type="file"
                    accept=".bib,.bibtex,text/x-bibtex,text/plain"
                    multiple
                    node_ref=file_input
                    on:change=import_files
                />
                <button on:click=move |_| {
                    if let Some(el) = file_input.get_untracked() {
                        el.click();
                    }
                }>{move || state.t().import_bib}</button>
                <button on:click=new_entry>{move || state.t().new_entry}</button>
                <button class="primary" on:click=export>
                    {move || if count() > 0 { state.t().export_selected } else { state.t().export_bibtex }}
                </button>
            </Show>
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
