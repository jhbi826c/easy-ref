use crate::app::{AppState, Level};
use crate::bibtex;
use crate::model::Entry;
use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::{JsCast, JsValue};

#[derive(Clone, Debug, PartialEq)]
pub struct Export {
    pub ids: Vec<String>,
    pub name: String,
}

#[component]
pub fn ExportDialog(export: Export) -> impl IntoView {
    let state = expect_context::<AppState>();
    let (bib, problems, count) = state.lib.with_untracked(|l| {
        let mut entries: Vec<&Entry> = export.ids.iter().filter_map(|id| l.entry(id)).collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        (bibtex::to_bibtex(&entries), bibtex::check(&entries), entries.len())
    });
    let filename: String = export
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let filename = StoredValue::new(format!("{filename}.bib"));
    let bib = StoredValue::new(bib);
    let close = move |_| state.exporting.set(None);

    let copy = move |_| {
        let promise = window().navigator().clipboard().write_text(&bib.get_value());
        spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => state.notify(Level::Ok, "已复制到剪贴板"),
                Err(_) => state.notify(Level::Error, "复制失败，请手动选择文本复制"),
            }
        });
    };
    let download = move |_| match download_text(&filename.get_value(), &bib.get_value()) {
        Ok(()) => {}
        Err(_) => state.notify(Level::Error, "下载失败"),
    };

    view! {
        <div class="backdrop" on:click=close>
            <div class="modal" on:click=|ev: MouseEvent| ev.stop_propagation()>
                <header>
                    <h2>"导出 BibTeX"</h2>
                    <span class="muted">{format!("{count} 条 · {}", export.name)}</span>
                    <div class="spacer"></div>
                    <button class="icon" on:click=close>
                        "×"
                    </button>
                </header>
                {(!problems.is_empty())
                    .then(|| {
                        view! {
                            <div class="problems">
                                <strong>"以下条目可能让 BibTeX 产生警告，建议先补全："</strong>
                                <ul>{problems.into_iter().map(|p| view! { <li>{p}</li> }).collect_view()}</ul>
                            </div>
                        }
                    })}
                <textarea class="bib-output" readonly prop:value=bib.get_value()></textarea>
                <footer>
                    <div class="spacer"></div>
                    <button on:click=copy>"复制"</button>
                    <button class="primary" on:click=download>
                        {format!("下载 {}", filename.get_value())}
                    </button>
                </footer>
            </div>
        </div>
    }
}

fn download_text(filename: &str, text: &str) -> Result<(), JsValue> {
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/x-bibtex;charset=utf-8");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let a: web_sys::HtmlAnchorElement = document().create_element("a")?.dyn_into()?;
    a.set_href(&url);
    a.set_download(filename);
    a.click();
    web_sys::Url::revoke_object_url(&url)
}
