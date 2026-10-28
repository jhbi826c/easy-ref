use crate::app::{AppState, Level};
use crate::bibtex;
use crate::i18n::Strings;
use crate::metadata;
use crate::model::{Entry, EntryType, Person, normalize_doi};
use crate::storage::now;
use leptos::ev::{Event, MouseEvent};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashSet;

type Label = fn(&Strings) -> &'static str;

fn parse_authors(text: &str) -> Vec<Person> {
    text.lines().filter_map(Person::from_line).collect()
}

/// Local date as `YYYY-MM-DD`.
pub fn today() -> String {
    let d = js_sys::Date::new_0();
    format!("{:04}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date())
}

fn doi_url(doi: &str) -> Option<String> {
    crate::doi::parse_token(doi).map(|d| format!("https://doi.org/{d}"))
}

fn web_url(url: &str) -> Option<String> {
    (url.starts_with("https://") || url.starts_with("http://")).then(|| url.to_string())
}

fn authors_text(e: &Entry) -> String {
    e.authors.iter().map(Person::to_line).collect::<Vec<_>>().join("\n")
}

#[component]
pub fn EntryEditor(id: String) -> impl IntoView {
    let state = expect_context::<AppState>();
    let Some(original) = state.lib.with_untracked(|l| l.entry(&id).cloned()) else {
        return ().into_any();
    };
    let t = move || state.t();
    let id = StoredValue::new(id);
    let draft = RwSignal::new(original.clone());
    let authors = RwSignal::new(authors_text(&original));
    let year = RwSignal::new(original.year.map(|y| y.to_string()).unwrap_or_default());

    let current = move || {
        let mut e = draft.get();
        e.authors = parse_authors(&authors.get());
        e.year = year.get().trim().parse().ok();
        e
    };
    let save = move |_| {
        let mut e = current();
        e.key = bibtex::sanitize_key(&e.key);
        if e.key.is_empty() {
            e.key = bibtex::cite_key(&e);
        }
        state.lib.update(|l| l.update_entry(e, now()));
        if let Some(saved) = state.lib.with_untracked(|l| l.entry(&id.get_value()).cloned()) {
            authors.set(authors_text(&saved));
            draft.set(saved);
        }
        state.notify(Level::Ok, state.t_untracked().saved);
    };
    let regenerate_key = move |_| {
        let key = bibtex::cite_key(&current());
        draft.update(|d| d.key = key);
    };
    let delete = move |_| state.delete_entries(vec![id.get_value()]);
    let trashed = move || {
        state
            .lib
            .with(|l| l.entry(&id.get_value()).is_some_and(|e| e.trashed.is_some()))
    };
    let restore = move |_| {
        state.lib.update(|l| l.restore_entries(&[id.get_value()], now()));
        state.notify(Level::Ok, (state.t_untracked().restored)(1));
    };

    let fetching = RwSignal::new(false);
    // Replaces the entry's metadata with what the DOI resolves to.
    let refresh_from_doi = move |raw: String, keep_key: bool| {
        let Some(doi) = crate::doi::parse_token(raw.trim()) else {
            return;
        };
        let entry_id = id.get_value();
        let t = state.t_untracked();
        let taken = state
            .lib
            .with_untracked(|l| l.find_by_doi(&doi).filter(|e| e.id != entry_id).map(|e| e.key.clone()));
        if let Some(other) = taken {
            return state.notify(Level::Warn, (t.doi_taken)(&other));
        }
        fetching.set(true);
        spawn_local(async move {
            let result = metadata::lookup(&doi).await;
            let _ = fetching.try_set(false);
            let mut fetched = match result {
                Ok(e) => e,
                Err(e) => return state.notify(Level::Error, (t.lookup_failed)(&doi, &e)),
            };
            let Some(current) = state.lib.with_untracked(|l| l.entry(&entry_id).cloned()) else {
                return;
            };
            fetched.id = current.id.clone();
            fetched.added = current.added;
            fetched.trashed = current.trashed;
            fetched.key = if keep_key {
                current.key.clone()
            } else {
                bibtex::cite_key(&fetched)
            };
            state.lib.update(|l| l.update_entry(fetched, now()));
            if let Some(saved) = state.lib.with_untracked(|l| l.entry(&entry_id).cloned()) {
                let _ = authors.try_set(authors_text(&saved));
                let _ = year.try_set(saved.year.map(|y| y.to_string()).unwrap_or_default());
                state.notify(Level::Ok, (t.updated_from_doi)(&current.key, &saved.key));
                let _ = draft.try_set(saved);
            }
        });
    };
    let on_doi_change = move |ev: Event| {
        let value = event_target_value(&ev);
        let saved = state.lib.with_untracked(|l| {
            l.entry(&id.get_value())
                .map(|e| normalize_doi(&e.doi))
                .unwrap_or_default()
        });
        if crate::doi::parse_token(value.trim()).is_some_and(|d| normalize_doi(&d) != saved) {
            refresh_from_doi(value, false);
        }
    };

    let text_field = move |label: Label, get: fn(&Entry) -> &String, set: fn(&mut Entry, String)| {
        view! {
            <label class="field">
                <span>{move || label(t())}</span>
                <input
                    prop:value=move || draft.with(|d| get(d).clone())
                    on:input=move |ev| draft.update(|d| set(d, event_target_value(&ev)))
                />
            </label>
        }
    };
    let link_field = move |label: &'static str,
                           get: fn(&Entry) -> &String,
                           set: fn(&mut Entry, String),
                           href: fn(&str) -> Option<String>| {
        let target = move || draft.with(|d| href(get(d).trim()));
        view! {
            <label class="field">
                <span>{label}</span>
                <div class="row">
                    <input
                        prop:value=move || draft.with(|d| get(d).clone())
                        on:input=move |ev| draft.update(|d| set(d, event_target_value(&ev)))
                    />
                    {move || {
                        target()
                            .map(|url| {
                                view! {
                                    <a class="button" href=url target="_blank" rel="noopener" title=move || t().open_link>
                                        "↗"
                                    </a>
                                }
                            })
                    }}
                </div>
            </label>
        }
    };

    let memberships = move || {
        state.lib.with(|l| {
            let entry_id = id.get_value();
            let direct: HashSet<&str> = l
                .collections
                .iter()
                .filter(|c| c.entries.contains(&entry_id))
                .map(|c| c.id.as_str())
                .collect();
            l.tree()
                .into_iter()
                .map(|(c, depth)| {
                    let cid = c.id.clone();
                    let is_direct = direct.contains(c.id.as_str());
                    let via_child = !is_direct && l.descendants(&c.id).iter().any(|d| direct.contains(d.as_str()));
                    // Unticking removes the entry from this collection and everything below it.
                    let on_change = move |ev: Event| {
                        let ids = [id.get_value()];
                        if event_target_checked(&ev) {
                            state.lib.update(|l| l.set_membership(&cid, &ids[0], true, now()));
                        } else {
                            state.lib.update(|l| l.remove_from_collection(&cid, &ids, now()));
                        }
                    };
                    view! {
                        <label
                            class="check"
                            class:implied=via_child
                            style=format!("padding-left: {}px", depth * 18)
                            title=move || if via_child { t().in_subcollection } else { "" }
                        >
                            <input type="checkbox" prop:checked=is_direct || via_child on:change=on_change />
                            {c.name.clone()}
                        </label>
                    }
                })
                .collect_view()
        })
    };

    view! {
        <aside class="editor">
            <header>
                <h2>{move || t().edit_entry}</h2>
                <button class="icon" title=move || t().close on:click=move |_| state.editing.set(None)>
                    "×"
                </button>
            </header>
            <div class="editor-body">
                <label class="field">
                    <span>{move || t().field_key}</span>
                    <div class="row">
                        <input
                            class="mono"
                            prop:value=move || draft.with(|d| d.key.clone())
                            on:input=move |ev| draft.update(|d| d.key = event_target_value(&ev))
                        />
                        <button on:click=regenerate_key title=move || t().regenerate_hint>
                            {move || t().regenerate}
                        </button>
                    </div>
                </label>
                <label class="field">
                    <span>{move || t().field_type}</span>
                    <select on:change=move |ev| {
                        let v = event_target_value(&ev);
                        if let Some(kind) = EntryType::from_bibtex_name(&v) {
                            draft.update(|d| {
                                d.kind = kind;
                                if kind == EntryType::Online && d.urldate.trim().is_empty() {
                                    d.urldate = today();
                                }
                            });
                        }
                    }>
                        {EntryType::ALL
                            .into_iter()
                            .map(|kind| {
                                view! {
                                    <option value=kind.bibtex_name() selected=move || draft.with(|d| d.kind == kind)>
                                        {move || format!("{} (@{})", (t().entry_type)(kind), kind.bibtex_name())}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                {text_field(|t| t.field_title, |e| &e.title, |e, v| e.title = v)}
                <label class="field">
                    <span>{move || t().field_authors}</span>
                    <textarea
                        rows=4
                        prop:value=move || authors.get()
                        on:input=move |ev| authors.set(event_target_value(&ev))
                    ></textarea>
                </label>
                <div class="grid2">
                    <label class="field">
                        <span>{move || t().field_year}</span>
                        <input prop:value=move || year.get() on:input=move |ev| year.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>{move || t().field_month}</span>
                        <select on:change=move |ev| {
                            let m = event_target_value(&ev).parse().ok();
                            draft.update(|d| d.month = m);
                        }>
                            <option value="" selected=move || draft.with(|d| d.month.is_none())>
                                "—"
                            </option>
                            {(1..=12u8)
                                .map(|m| {
                                    view! {
                                        <option value=m.to_string() selected=move || draft.with(|d| d.month == Some(m))>
                                            {m.to_string()}
                                        </option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </label>
                </div>
                <label class="field">
                    <span>
                        {move || {
                            if draft.with(|d| d.kind == EntryType::Online) { t().field_website } else { t().field_venue }
                        }}
                    </span>
                    <input
                        prop:value=move || draft.with(|d| d.venue.clone())
                        on:input=move |ev| draft.update(|d| d.venue = event_target_value(&ev))
                    />
                </label>
                <div class="grid2">
                    {text_field(|t| t.field_venue_short, |e| &e.venue_short, |e, v| e.venue_short = v)}
                    {text_field(|t| t.field_series, |e| &e.series, |e, v| e.series = v)}
                </div>
                <div class="grid3">
                    {text_field(|t| t.field_volume, |e| &e.volume, |e, v| e.volume = v)}
                    {text_field(|t| t.field_number, |e| &e.number, |e, v| e.number = v)}
                    {text_field(|t| t.field_pages, |e| &e.pages, |e, v| e.pages = v)}
                </div>
                <div class="grid2">
                    {text_field(|t| t.field_publisher, |e| &e.publisher, |e, v| e.publisher = v)}
                    {text_field(|t| t.field_address, |e| &e.address, |e, v| e.address = v)}
                </div>
                <label class="field">
                    <span>"DOI"</span>
                    <div class="row">
                        <input
                            prop:value=move || draft.with(|d| d.doi.clone())
                            on:input=move |ev| draft.update(|d| d.doi = event_target_value(&ev))
                            on:change=on_doi_change
                        />
                        {move || {
                            draft
                                .with(|d| doi_url(d.doi.trim()))
                                .map(|url| {
                                    view! {
                                        <a class="button" href=url target="_blank" rel="noopener" title=move || t().open_link>
                                            "↗"
                                        </a>
                                        <button
                                            title=move || t().refresh_from_doi
                                            disabled=move || fetching.get()
                                            on:click=move |ev: MouseEvent| {
                                                ev.prevent_default();
                                                refresh_from_doi(draft.with_untracked(|d| d.doi.clone()), true);
                                            }
                                        >
                                            {move || if fetching.get() { "…" } else { "⟳" }}
                                        </button>
                                    }
                                })
                        }}
                    </div>
                    <Show when=move || fetching.get()>
                        <span class="muted small">{move || t().fetching_doi}</span>
                    </Show>
                </label>
                <div class="grid-url">
                    {link_field("URL", |e| &e.url, |e, v| e.url = v, web_url)}
                    <label class="field">
                        <span>{move || t().field_urldate}</span>
                        <input
                            type="date"
                            prop:value=move || draft.with(|d| d.urldate.clone())
                            on:input=move |ev| draft.update(|d| d.urldate = event_target_value(&ev))
                        />
                    </label>
                </div>
                <div class="grid2">
                    {text_field(|t| t.field_arxiv_id, |e| &e.eprint, |e, v| e.eprint = v)}
                    {text_field(|t| t.field_arxiv_class, |e| &e.primary_class, |e, v| e.primary_class = v)}
                </div>
                <div class="field">
                    <span>{move || t().field_collections}</span>
                    <div class="checks">{memberships}</div>
                </div>
                <div class="field">
                    <span>{move || t().bibtex_preview}</span>
                    <pre class="bib-preview">{move || bibtex::entry_to_bibtex(&current())}</pre>
                </div>
            </div>
            <footer>
                <button class="danger" on:click=delete>
                    {move || if trashed() { t().delete_forever } else { t().delete }}
                </button>
                <Show when=trashed>
                    <button on:click=restore>{move || t().restore}</button>
                </Show>
                <div class="spacer"></div>
                <button class="primary" on:click=save>
                    {move || t().save}
                </button>
            </footer>
        </aside>
    }
    .into_any()
}
