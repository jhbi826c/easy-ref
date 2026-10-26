use crate::bibtex::Problem;
use crate::metadata::LookupError;
use crate::model::EntryType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Zh, Lang::En];

    pub fn from_browser(language: &str) -> Lang {
        if language.to_lowercase().starts_with("zh") { Lang::Zh } else { Lang::En }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::Zh => "中文",
            Lang::En => "English",
        }
    }

    pub fn html_tag(self) -> &'static str {
        match self {
            Lang::Zh => "zh-CN",
            Lang::En => "en",
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::Zh => &ZH,
            Lang::En => &EN,
        }
    }
}

pub struct Strings {
    pub library: &'static str,
    pub collections: &'static str,
    pub new_collection: &'static str,
    pub collection_name: &'static str,
    pub collections_hint: &'static str,
    pub rename_hint: &'static str,
    pub delete_collection: &'static str,
    pub confirm_delete_collection: fn(&str) -> String,

    pub add_placeholder: &'static str,
    pub add: &'static str,
    pub looking_up: fn(usize) -> String,
    pub unrecognized: fn(&str) -> String,
    pub lookup_failed: fn(&str, &LookupError) -> String,
    pub added_n: fn(usize) -> String,
    pub dup_in_library: fn(&str) -> String,
    pub dup_added_to_collection: fn(&str) -> String,
    pub dup_in_collection: fn(&str) -> String,
    pub added_to_collection: fn(usize, &str) -> String,

    pub n_entries: fn(usize) -> String,
    pub n_selected: fn(usize) -> String,
    pub search_placeholder: &'static str,
    pub add_to_collection: &'static str,
    pub remove_from_collection: &'static str,
    pub delete: &'static str,
    pub confirm_delete_entries: fn(usize) -> String,
    pub new_entry: &'static str,
    pub export_bibtex: &'static str,
    pub export_selected: &'static str,

    pub col_title: &'static str,
    pub col_authors: &'static str,
    pub col_year: &'static str,
    pub col_venue: &'static str,
    pub col_type: &'static str,
    pub col_key: &'static str,
    pub col_added: &'static str,
    pub untitled: &'static str,
    pub empty_title: &'static str,
    pub empty_hint: &'static str,
    pub no_match: &'static str,
    pub entry_type: fn(EntryType) -> &'static str,
    pub preprint: &'static str,
    pub other: &'static str,

    pub edit_entry: &'static str,
    pub close: &'static str,
    pub field_key: &'static str,
    pub regenerate: &'static str,
    pub regenerate_hint: &'static str,
    pub field_type: &'static str,
    pub field_title: &'static str,
    pub field_authors: &'static str,
    pub field_year: &'static str,
    pub field_month: &'static str,
    pub field_venue: &'static str,
    pub field_venue_short: &'static str,
    pub field_series: &'static str,
    pub field_volume: &'static str,
    pub field_number: &'static str,
    pub field_pages: &'static str,
    pub field_publisher: &'static str,
    pub field_address: &'static str,
    pub field_arxiv_id: &'static str,
    pub field_arxiv_class: &'static str,
    pub field_collections: &'static str,
    pub bibtex_preview: &'static str,
    pub save: &'static str,
    pub saved: &'static str,
    pub confirm_delete_entry: &'static str,

    pub export_title: &'static str,
    pub problems_title: &'static str,
    pub problem: fn(&Problem) -> String,
    pub copy: &'static str,
    pub copied: &'static str,
    pub copy_failed: &'static str,
    pub download: fn(&str) -> String,
    pub download_failed: &'static str,
}

pub const ZH: Strings = Strings {
    library: "全部文献",
    collections: "目录",
    new_collection: "新建目录",
    collection_name: "目录名称",
    collections_hint: "点 + 新建目录，把条目拖到目录上即可加入。",
    rename_hint: "双击重命名",
    delete_collection: "删除目录",
    confirm_delete_collection: |name| format!("删除目录「{name}」？条目本身不会被删除。"),

    add_placeholder: "粘贴 DOI / 链接 / arXiv 编号，可一次多个，回车添加",
    add: "添加",
    looking_up: |n| format!("正在查询 {n} 条…"),
    unrecognized: |raw| format!("无法识别：{raw}（IEEE Xplore 等链接请复制页面上的 DOI）"),
    lookup_failed: |doi, e| match e {
        LookupError::NotFound => format!("找不到 DOI：{doi}"),
        LookupError::Failed(msg) => format!("{doi}：请求失败（可能被限流，请稍后重试）：{msg}"),
        LookupError::Http(code) => format!("{doi}：服务器返回 HTTP {code}"),
        LookupError::Parse(msg) => format!("{doi}：无法解析返回数据：{msg}"),
    },
    added_n: |n| format!("已添加 {n} 条"),
    dup_in_library: |key| format!("{key} 已在文献库中，未重复添加"),
    dup_added_to_collection: |key| format!("{key} 已在文献库中，已加入当前目录"),
    dup_in_collection: |key| format!("{key} 已在当前目录中"),
    added_to_collection: |n, name| format!("{n} 条加入「{name}」"),

    n_entries: |n| format!("{n} 条"),
    n_selected: |n| format!("已选 {n} 条"),
    search_placeholder: "搜索标题 / 作者 / 期刊 / key",
    add_to_collection: "加入目录…",
    remove_from_collection: "移出目录",
    delete: "删除",
    confirm_delete_entries: |n| format!("从文献库彻底删除 {n} 条？所有目录中都会移除。"),
    new_entry: "手动新建",
    export_bibtex: "导出 BibTeX",
    export_selected: "导出所选 BibTeX",

    col_title: "标题",
    col_authors: "作者",
    col_year: "年份",
    col_venue: "期刊 / 会议",
    col_type: "类型",
    col_key: "引用 key",
    col_added: "添加时间",
    untitled: "（无标题）",
    empty_title: "这里还没有条目",
    empty_hint: "在上方粘贴 DOI、doi.org / ACM / Springer 链接或 arXiv 编号，回车即可添加。",
    no_match: "没有匹配的条目",
    entry_type: |t| match t {
        EntryType::Article => "期刊",
        EntryType::InProceedings => "会议",
        EntryType::InCollection => "书章节",
        EntryType::Book => "书",
        EntryType::PhdThesis => "博士论文",
        EntryType::TechReport => "技术报告",
        EntryType::Misc => "预印本 / 其他",
    },
    preprint: "预印本",
    other: "其他",

    edit_entry: "编辑条目",
    close: "关闭 (Esc)",
    field_key: "引用 key",
    regenerate: "重新生成",
    regenerate_hint: "按 一作姓 + 年份 + 期刊/会议简称 重新生成",
    field_type: "类型",
    field_title: "标题",
    field_authors: "作者（每行一位：姓, 名；机构用 {名称}）",
    field_year: "年份",
    field_month: "月份",
    field_venue: "期刊 / 会议 / 书名",
    field_venue_short: "简称（用于 key）",
    field_series: "丛书 series",
    field_volume: "卷 volume",
    field_number: "期 number",
    field_pages: "页码 pages",
    field_publisher: "出版社 / 学校 / 机构",
    field_address: "出版地 address",
    field_arxiv_id: "arXiv 编号",
    field_arxiv_class: "arXiv 分类",
    field_collections: "所属目录",
    bibtex_preview: "BibTeX 预览",
    save: "保存",
    saved: "已保存",
    confirm_delete_entry: "从文献库彻底删除这个条目？",

    export_title: "导出 BibTeX",
    problems_title: "以下条目可能让 BibTeX 产生警告，建议先补全：",
    problem: |p| match p {
        Problem::NoKey { title } => format!("「{title}」没有引用 key"),
        Problem::Missing { key, fields } => format!("{key}：缺少 {}", fields.join(", ")),
        Problem::DuplicateKey(key) => format!("{key}：key 重复"),
    },
    copy: "复制",
    copied: "已复制到剪贴板",
    copy_failed: "复制失败，请手动选择文本复制",
    download: |name| format!("下载 {name}"),
    download_failed: "下载失败",
};

pub const EN: Strings = Strings {
    library: "Library",
    collections: "Collections",
    new_collection: "New collection",
    collection_name: "Collection name",
    collections_hint: "Click + to create a collection, then drag entries onto it.",
    rename_hint: "Double-click to rename",
    delete_collection: "Delete collection",
    confirm_delete_collection: |name| format!("Delete collection \"{name}\"? Its entries stay in the library."),

    add_placeholder: "Paste DOIs, links or arXiv IDs (several at once), press Enter",
    add: "Add",
    looking_up: |n| format!("Looking up {n}…"),
    unrecognized: |raw| format!("Not recognized: {raw} (for IEEE Xplore and similar links, copy the DOI from the page)"),
    lookup_failed: |doi, e| match e {
        LookupError::NotFound => format!("DOI not found: {doi}"),
        LookupError::Failed(msg) => format!("{doi}: request failed (possibly rate limited, try again later): {msg}"),
        LookupError::Http(code) => format!("{doi}: server returned HTTP {code}"),
        LookupError::Parse(msg) => format!("{doi}: could not parse response: {msg}"),
    },
    added_n: |n| format!("Added {n} {}", if n == 1 { "entry" } else { "entries" }),
    dup_in_library: |key| format!("{key} is already in the library, skipped"),
    dup_added_to_collection: |key| format!("{key} is already in the library, added to this collection"),
    dup_in_collection: |key| format!("{key} is already in this collection"),
    added_to_collection: |n, name| format!("Added {n} to \"{name}\""),

    n_entries: |n| format!("{n} {}", if n == 1 { "entry" } else { "entries" }),
    n_selected: |n| format!("{n} selected"),
    search_placeholder: "Search title / author / venue / key",
    add_to_collection: "Add to collection…",
    remove_from_collection: "Remove from collection",
    delete: "Delete",
    confirm_delete_entries: |n| format!("Permanently delete {n} entries from the library and all collections?"),
    new_entry: "New entry",
    export_bibtex: "Export BibTeX",
    export_selected: "Export selected",

    col_title: "Title",
    col_authors: "Authors",
    col_year: "Year",
    col_venue: "Venue",
    col_type: "Type",
    col_key: "Key",
    col_added: "Added",
    untitled: "(untitled)",
    empty_title: "No entries yet",
    empty_hint: "Paste a DOI, a doi.org / ACM / Springer link or an arXiv ID above and press Enter.",
    no_match: "No matching entries",
    entry_type: |t| match t {
        EntryType::Article => "Journal",
        EntryType::InProceedings => "Conference",
        EntryType::InCollection => "Chapter",
        EntryType::Book => "Book",
        EntryType::PhdThesis => "PhD thesis",
        EntryType::TechReport => "Report",
        EntryType::Misc => "Preprint / other",
    },
    preprint: "Preprint",
    other: "Other",

    edit_entry: "Edit entry",
    close: "Close (Esc)",
    field_key: "Citation key",
    regenerate: "Regenerate",
    regenerate_hint: "Rebuild from first author + year + venue abbreviation",
    field_type: "Type",
    field_title: "Title",
    field_authors: "Authors (one per line: Last, First; organizations as {Name})",
    field_year: "Year",
    field_month: "Month",
    field_venue: "Journal / proceedings / book",
    field_venue_short: "Abbreviation (used in key)",
    field_series: "Series",
    field_volume: "Volume",
    field_number: "Number",
    field_pages: "Pages",
    field_publisher: "Publisher / school / institution",
    field_address: "Address",
    field_arxiv_id: "arXiv ID",
    field_arxiv_class: "arXiv category",
    field_collections: "Collections",
    bibtex_preview: "BibTeX preview",
    save: "Save",
    saved: "Saved",
    confirm_delete_entry: "Permanently delete this entry from the library?",

    export_title: "Export BibTeX",
    problems_title: "These entries may cause BibTeX warnings; consider completing them first:",
    problem: |p| match p {
        Problem::NoKey { title } => format!("\"{title}\" has no citation key"),
        Problem::Missing { key, fields } => format!("{key}: missing {}", fields.join(", ")),
        Problem::DuplicateKey(key) => format!("{key}: duplicate key"),
    },
    copy: "Copy",
    copied: "Copied to clipboard",
    copy_failed: "Copy failed, please select the text and copy it manually",
    download: |name| format!("Download {name}"),
    download_failed: "Download failed",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_browser_language() {
        assert_eq!(Lang::from_browser("zh-CN"), Lang::Zh);
        assert_eq!(Lang::from_browser("zh-TW"), Lang::Zh);
        assert_eq!(Lang::from_browser("en-US"), Lang::En);
        assert_eq!(Lang::from_browser("de"), Lang::En);
    }

    #[test]
    fn formats_messages() {
        assert_eq!((EN.added_n)(1), "Added 1 entry");
        assert_eq!((ZH.added_n)(3), "已添加 3 条");
        let p = Problem::Missing { key: "k".into(), fields: vec!["author", "year"] };
        assert_eq!((EN.problem)(&p), "k: missing author, year");
    }
}
