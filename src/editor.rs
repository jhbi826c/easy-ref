use crate::app::{AppState, Level};
use crate::bibtex;
use crate::model::{Entry, EntryType, Person};
use crate::storage::now;
use leptos::ev::Event;
use leptos::prelude::*;

fn parse_authors(text: &str) -> Vec<Person> {
    text.lines().filter_map(Person::from_line).collect()
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
        state.notify(Level::Ok, "已保存");
    };
    let regenerate_key = move |_| {
        let key = bibtex::cite_key(&current());
        draft.update(|d| d.key = key);
    };
    let delete = move |_| {
        if window().confirm_with_message("从文献库彻底删除这个条目？").unwrap_or(false) {
            state.lib.update(|l| l.delete_entries(&[id.get_value()], now()));
            state.editing.set(None);
        }
    };

    let text_field = move |label: &'static str, get: fn(&Entry) -> &String, set: fn(&mut Entry, String)| {
        view! {
            <label class="field">
                <span>{label}</span>
                <input
                    prop:value=move || draft.with(|d| get(d).clone())
                    on:input=move |ev| draft.update(|d| set(d, event_target_value(&ev)))
                />
            </label>
        }
    };

    let memberships = move || {
        state.lib.with(|l| {
            l.collections
                .iter()
                .map(|c| {
                    let cid = c.id.clone();
                    let member = c.entries.contains(&id.get_value());
                    let on_change = move |ev: Event| {
                        let ids = [id.get_value()];
                        if event_target_checked(&ev) {
                            state.lib.update(|l| {
                                l.add_to_collection(&cid, &ids, now());
                            });
                        } else {
                            state.lib.update(|l| l.remove_from_collection(&cid, &ids, now()));
                        }
                    };
                    view! {
                        <label class="check">
                            <input type="checkbox" prop:checked=member on:change=on_change />
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
                <h2>"编辑条目"</h2>
                <button class="icon" title="关闭 (Esc)" on:click=move |_| state.editing.set(None)>
                    "×"
                </button>
            </header>
            <div class="editor-body">
                <label class="field">
                    <span>"引用 key"</span>
                    <div class="row">
                        <input
                            class="mono"
                            prop:value=move || draft.with(|d| d.key.clone())
                            on:input=move |ev| draft.update(|d| d.key = event_target_value(&ev))
                        />
                        <button on:click=regenerate_key title="按 一作姓+年份+期刊/会议 重新生成">
                            "重新生成"
                        </button>
                    </div>
                </label>
                <label class="field">
                    <span>"类型"</span>
                    <select on:change=move |ev| {
                        let v = event_target_value(&ev);
                        if let Some(kind) = EntryType::from_bibtex_name(&v) {
                            draft.update(|d| d.kind = kind);
                        }
                    }>
                        {EntryType::ALL
                            .into_iter()
                            .map(|t| {
                                view! {
                                    <option
                                        value=t.bibtex_name()
                                        selected=move || draft.with(|d| d.kind == t)
                                    >
                                        {format!("{} (@{})", t.label(), t.bibtex_name())}
                                    </option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                {text_field("标题", |e| &e.title, |e, v| e.title = v)}
                <label class="field">
                    <span>"作者（每行一位：姓, 名；机构用 {名称}）"</span>
                    <textarea
                        rows=4
                        prop:value=move || authors.get()
                        on:input=move |ev| authors.set(event_target_value(&ev))
                    ></textarea>
                </label>
                <div class="grid2">
                    <label class="field">
                        <span>"年份"</span>
                        <input prop:value=move || year.get() on:input=move |ev| year.set(event_target_value(&ev)) />
                    </label>
                    <label class="field">
                        <span>"月份"</span>
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
                                        <option
                                            value=m.to_string()
                                            selected=move || draft.with(|d| d.month == Some(m))
                                        >
                                            {m.to_string()}
                                        </option>
                                    }
                                })
                                .collect_view()}
                        </select>
                    </label>
                </div>
                {text_field("期刊 / 会议 / 书名", |e| &e.venue, |e, v| e.venue = v)}
                <div class="grid2">
                    {text_field("简称（用于 key）", |e| &e.venue_short, |e, v| e.venue_short = v)}
                    {text_field("丛书 series", |e| &e.series, |e, v| e.series = v)}
                </div>
                <div class="grid3">
                    {text_field("卷 volume", |e| &e.volume, |e, v| e.volume = v)}
                    {text_field("期 number", |e| &e.number, |e, v| e.number = v)}
                    {text_field("页码 pages", |e| &e.pages, |e, v| e.pages = v)}
                </div>
                <div class="grid2">
                    {text_field("出版社 / 学校 / 机构", |e| &e.publisher, |e, v| e.publisher = v)}
                    {text_field("出版地 address", |e| &e.address, |e, v| e.address = v)}
                </div>
                {text_field("DOI", |e| &e.doi, |e, v| e.doi = v)}
                {text_field("URL", |e| &e.url, |e, v| e.url = v)}
                <div class="grid2">
                    {text_field("arXiv eprint", |e| &e.eprint, |e, v| e.eprint = v)}
                    {text_field("arXiv 分类", |e| &e.primary_class, |e, v| e.primary_class = v)}
                </div>
                <div class="field">
                    <span>"所属目录"</span>
                    <div class="checks">{memberships}</div>
                </div>
                <div class="field">
                    <span>"BibTeX 预览"</span>
                    <pre class="bib-preview">{move || bibtex::entry_to_bibtex(&current())}</pre>
                </div>
            </div>
            <footer>
                <button class="danger" on:click=delete>
                    "删除"
                </button>
                <div class="spacer"></div>
                <button class="primary" on:click=save>
                    "保存"
                </button>
            </footer>
        </aside>
    }
    .into_any()
}
