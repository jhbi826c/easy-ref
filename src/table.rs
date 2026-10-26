use crate::app::AppState;
use crate::i18n::Strings;
use crate::model::{Entry, EntryType};
use leptos::ev::{DragEvent, MouseEvent};
use leptos::prelude::*;
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortColumn {
    Title,
    Authors,
    Year,
    Venue,
    Type,
    Key,
    Added,
}

type Label = fn(&Strings) -> &'static str;

const COLUMNS: [(SortColumn, Label, &str); 7] = [
    (SortColumn::Title, |t| t.col_title, "col-title"),
    (SortColumn::Authors, |t| t.col_authors, "col-authors"),
    (SortColumn::Year, |t| t.col_year, "col-year"),
    (SortColumn::Venue, |t| t.col_venue, "col-venue"),
    (SortColumn::Type, |t| t.col_type, "col-type"),
    (SortColumn::Key, |t| t.col_key, "col-key"),
    (SortColumn::Added, |t| t.col_added, "col-added"),
];

pub fn sort_entries(entries: &mut [Entry], column: SortColumn, ascending: bool) {
    let text = |s: &str| s.to_lowercase();
    entries.sort_by(|a, b| {
        let ord = match column {
            SortColumn::Title => text(&a.title).cmp(&text(&b.title)),
            SortColumn::Authors => text(a.first_author_family()).cmp(&text(b.first_author_family())),
            SortColumn::Year => a.year.cmp(&b.year),
            SortColumn::Venue => text(&a.venue).cmp(&text(&b.venue)),
            SortColumn::Type => a.kind.cmp(&b.kind),
            SortColumn::Key => text(&a.key).cmp(&text(&b.key)),
            SortColumn::Added => a.added.partial_cmp(&b.added).unwrap_or(Ordering::Equal),
        };
        let ord = if ascending { ord } else { ord.reverse() };
        ord.then_with(|| a.key.cmp(&b.key))
    });
}

fn matches(e: &Entry, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {} {} {}",
        e.title,
        e.authors.iter().map(|p| p.display()).collect::<Vec<_>>().join(" "),
        e.venue,
        e.venue_short,
        e.key,
        e.doi
    )
    .to_lowercase();
    query.split_whitespace().all(|w| haystack.contains(w))
}

#[component]
pub fn EntryTable() -> impl IntoView {
    let state = expect_context::<AppState>();
    let rows = Memo::new(move |_| {
        let query = state.search.get().to_lowercase();
        let (column, ascending) = state.sort.get();
        let view = state.view.get();
        let mut rows: Vec<Entry> = state.lib.with(|l| {
            l.entries_in(&view)
                .into_iter()
                .filter(|e| matches(e, &query))
                .cloned()
                .collect()
        });
        sort_entries(&mut rows, column, ascending);
        rows
    });
    let anchor = RwSignal::new(None::<String>);

    let all_selected =
        move || rows.with(|r| !r.is_empty() && state.selected.with(|s| r.iter().all(|e| s.contains(&e.id))));
    let toggle_all = move |_| {
        if all_selected() {
            state.selected.set(HashSet::new());
        } else {
            state
                .selected
                .set(rows.with_untracked(|r| r.iter().map(|e| e.id.clone()).collect()));
        }
    };

    let headers = COLUMNS
        .into_iter()
        .map(|(column, label, class)| {
            let indicator = move || match state.sort.get() {
                (c, true) if c == column => "▲",
                (c, false) if c == column => "▼",
                _ => "",
            };
            let on_click = move |_| {
                state.sort.update(|(c, asc)| {
                    if *c == column {
                        *asc = !*asc;
                    } else {
                        *c = column;
                        *asc = !matches!(column, SortColumn::Year | SortColumn::Added);
                    }
                })
            };
            view! {
                <th class=class on:click=on_click>
                    {move || label(state.t())}
                    <span class="sort">{indicator}</span>
                </th>
            }
        })
        .collect_view();

    view! {
        <div class="table-wrap">
            <table class="entries">
                <thead>
                    <tr>
                        <th class="col-check">
                            <input type="checkbox" prop:checked=all_selected on:change=toggle_all />
                        </th>
                        {headers}
                    </tr>
                </thead>
                <tbody>
                    <For
                        each=move || rows.get()
                        key=|e| (e.id.clone(), e.modified.to_bits())
                        children=move |entry| view! { <Row entry=entry rows=rows anchor=anchor /> }
                    />
                </tbody>
            </table>
            <Show when=move || rows.with(|r| r.is_empty())>
                <div class="empty">
                    <Show
                        when=move || state.search.with(|s| s.is_empty())
                        fallback=move || view! { <div class="empty-title">{move || state.t().no_match}</div> }
                    >
                        <div class="empty-title">{move || state.t().empty_title}</div>
                        <div>{move || state.t().empty_hint}</div>
                        <code>"10.1109/CVPR.2016.90    arXiv:1706.03762"</code>
                    </Show>
                </div>
            </Show>
        </div>
    }
}

#[component]
fn Row(entry: Entry, rows: Memo<Vec<Entry>>, anchor: RwSignal<Option<String>>) -> impl IntoView {
    let state = expect_context::<AppState>();
    let id = StoredValue::new(entry.id.clone());
    let is_selected = move || state.selected.with(|s| s.contains(&id.get_value()));

    let toggle = move || {
        let id = id.get_value();
        state.selected.update(|s| {
            if !s.remove(&id) {
                s.insert(id.clone());
            }
        });
        anchor.set(Some(id));
    };
    let on_click = move |ev: MouseEvent| {
        let id = id.get_value();
        if ev.shift_key() {
            let range = rows.with_untracked(|r| {
                let pos = |target: &str| r.iter().position(|e| e.id == target);
                let end = pos(&id)?;
                let start = anchor.get_untracked().and_then(|a| pos(&a)).unwrap_or(end);
                let (lo, hi) = (start.min(end), start.max(end));
                Some(r[lo..=hi].iter().map(|e| e.id.clone()).collect::<HashSet<_>>())
            });
            if let Some(range) = range {
                state.selected.set(range);
            }
        } else if ev.meta_key() || ev.ctrl_key() {
            toggle();
        } else {
            state.selected.set(HashSet::from([id.clone()]));
            anchor.set(Some(id.clone()));
            state.editing.set(Some(id));
        }
    };
    let on_dragstart = move |ev: DragEvent| {
        let id = id.get_value();
        let ids: Vec<String> = state.selected.with_untracked(|s| {
            if s.contains(&id) {
                s.iter().cloned().collect()
            } else {
                vec![id.clone()]
            }
        });
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &format!("{} 条", ids.len()));
            dt.set_effect_allowed("copy");
        }
        state.dragging.set(ids);
    };

    let authors_full = entry.authors.iter().map(|p| p.display()).collect::<Vec<_>>().join("; ");
    let year = entry.year.map(|y| y.to_string()).unwrap_or_default();
    let title = StoredValue::new(entry.title.clone());
    let shown_title = move || {
        let t = title.get_value();
        if t.is_empty() {
            state.t().untitled.to_string()
        } else {
            t
        }
    };
    let kind_class = format!("tag {}", type_class(entry.kind));
    let label_entry = StoredValue::new(entry.clone());

    view! {
        <tr
            class:selected=is_selected
            draggable="true"
            on:click=on_click
            on:dragstart=on_dragstart
            on:dragend=move |_| state.dragging.set(vec![])
        >
            <td class="col-check" on:click=|ev: MouseEvent| ev.stop_propagation()>
                <input type="checkbox" prop:checked=is_selected on:change=move |_| toggle() />
            </td>
            <td class="col-title" title=title.get_value()>
                {shown_title}
            </td>
            <td class="col-authors" title=authors_full>
                {entry.authors_display()}
            </td>
            <td class="col-year">{year}</td>
            <td class="col-venue" title=entry.venue.clone()>
                {(!entry.venue_short.is_empty())
                    .then(|| view! { <span class="badge">{entry.venue_short.clone()}</span> })}
                {(entry.venue != entry.venue_short).then(|| entry.venue.clone())}
            </td>
            <td class="col-type">
                <span class=kind_class>{move || label_entry.with_value(|e| type_label(state.t(), e))}</span>
            </td>
            <td class="col-key">{entry.key.clone()}</td>
            <td class="col-added">{format_date(entry.added)}</td>
        </tr>
    }
}

fn type_class(kind: EntryType) -> &'static str {
    match kind {
        EntryType::Article => "journal",
        EntryType::InProceedings => "conf",
        EntryType::Misc => "preprint",
        _ => "other",
    }
}

fn type_label(t: &'static Strings, e: &Entry) -> &'static str {
    match e.kind {
        EntryType::Misc if !e.eprint.is_empty() => t.preprint,
        EntryType::Misc => t.other,
        kind => (t.entry_type)(kind),
    }
}

fn format_date(ms: f64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms));
    format!("{:04}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Person;

    #[test]
    fn sorts_by_year_then_key() {
        let mk = |key: &str, year: i32, family: &str| Entry {
            key: key.into(),
            year: Some(year),
            authors: vec![Person {
                family: family.into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut v = vec![mk("b", 2020, "Zhang"), mk("a", 2020, "Adams"), mk("c", 2018, "Li")];
        sort_entries(&mut v, SortColumn::Year, false);
        assert_eq!(v.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        sort_entries(&mut v, SortColumn::Authors, true);
        assert_eq!(v.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["a", "c", "b"]);
    }
}
