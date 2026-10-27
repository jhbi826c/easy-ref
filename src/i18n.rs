use crate::bibtex::Problem;
use crate::metadata::LookupError;
use crate::model::EntryType;
use crate::settings::Theme;
use crate::sync::SyncError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Zh, Lang::En];

    pub fn from_browser(language: &str) -> Lang {
        if language.to_lowercase().starts_with("zh") {
            Lang::Zh
        } else {
            Lang::En
        }
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
    pub trash: &'static str,
    pub trash_empty: &'static str,
    pub restore: &'static str,
    pub select_to_restore: &'static str,
    pub delete_forever: &'static str,
    pub empty_trash: &'static str,
    pub confirm_empty_trash: fn(usize) -> String,
    pub moved_to_trash: fn(usize) -> String,
    pub restored: fn(usize) -> String,
    pub replaced: fn(&str, &str) -> String,
    pub collections: &'static str,
    pub new_collection: &'static str,
    pub new_subcollection: &'static str,
    pub collection_name: &'static str,
    pub collections_hint: &'static str,
    pub rename_hint: &'static str,
    pub rename: &'static str,
    pub resize_sidebar: &'static str,
    pub delete_collection: &'static str,
    pub confirm_delete_collection: fn(&str, usize) -> String,

    pub add_placeholder: &'static str,
    pub add: &'static str,
    pub import: &'static str,
    pub bib_detected: &'static str,
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
    pub import_bib: &'static str,
    pub imported: fn(usize, usize) -> String,
    pub bib_parse_error: fn(usize) -> String,
    pub no_bib_entries: &'static str,
    pub export_bibtex: &'static str,
    pub export_selected: &'static str,

    pub col_title: &'static str,
    pub col_authors: &'static str,
    pub col_year: &'static str,
    pub col_venue: &'static str,
    pub col_type: &'static str,
    pub col_key: &'static str,
    pub col_added: &'static str,
    pub col_deleted: &'static str,
    pub resize_column: &'static str,
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
    pub in_subcollection: &'static str,
    pub bibtex_preview: &'static str,
    pub save: &'static str,
    pub saved: &'static str,

    pub export_title: &'static str,
    pub problems_title: &'static str,
    pub problem: fn(&Problem) -> String,
    pub copy: &'static str,
    pub copied: &'static str,
    pub copy_failed: &'static str,
    pub download: fn(&str) -> String,
    pub download_failed: &'static str,

    pub settings: &'static str,
    pub appearance: &'static str,
    pub language: &'static str,
    pub theme_name: fn(Theme) -> &'static str,

    pub install_section: &'static str,
    pub install_button: &'static str,
    pub install_hint: &'static str,
    pub installed: &'static str,

    pub sync_section: &'static str,
    pub sync_intro: &'static str,
    pub field_token: &'static str,
    pub token_hint: &'static str,
    pub create_token: &'static str,
    pub open_gist: &'static str,
    pub open_link: &'static str,
    pub auto_sync: &'static str,
    pub save_and_sync: &'static str,
    pub disconnect: &'static str,
    pub sync_now: &'static str,
    pub syncing: &'static str,
    pub sync_done: &'static str,
    pub synced_at: fn(&str) -> String,
    pub unsynced: &'static str,
    pub sync_failed: &'static str,
    pub sync_error: fn(&SyncError) -> String,
}

pub const ZH: Strings = Strings {
    library: "全部文献",
    trash: "回收站",
    trash_empty: "回收站是空的",
    restore: "恢复",
    select_to_restore: "勾选条目后恢复，或把条目拖回「全部文献」或某个目录",
    delete_forever: "彻底删除",
    empty_trash: "清空回收站",
    confirm_empty_trash: |n| format!("彻底删除回收站中的 {n} 条？此操作无法撤销。"),
    moved_to_trash: |n| format!("{n} 条已移到回收站"),
    restored: |n| format!("已恢复 {n} 条"),
    replaced: |old, new| {
        if old == new {
            format!("{old} 已更新为新版本")
        } else {
            format!("{old} 已更新为新版本：{new}")
        }
    },
    collections: "目录",
    new_collection: "新建目录",
    new_subcollection: "新建子目录",
    collection_name: "目录名称",
    collections_hint: "点 + 新建目录，把条目拖到目录上即可加入。",
    rename_hint: "双击重命名；拖到其他目录的上下边缘调整顺序，拖到中间放进去",
    rename: "重命名",
    resize_sidebar: "拖动调整宽度，双击还原",
    delete_collection: "删除目录",
    confirm_delete_collection: |name, subfolders| {
        if subfolders == 0 {
            format!("删除目录「{name}」？条目本身不会被删除。")
        } else {
            format!("删除目录「{name}」及其中的 {subfolders} 个子目录？条目本身不会被删除。")
        }
    },

    add_placeholder: "粘贴 DOI / 链接 / arXiv 编号 / BibTeX，可一次多个，回车添加",
    add: "添加",
    import: "导入",
    bib_detected: "检测到 BibTeX：回车换行，⌘/Ctrl + 回车导入",
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
    confirm_delete_entries: |n| format!("彻底删除 {n} 条？此操作无法撤销。"),
    new_entry: "手动新建",
    import_bib: "导入 .bib",
    imported: |added, skipped| {
        if skipped == 0 {
            format!("已导入 {added} 条")
        } else {
            format!("已导入 {added} 条，{skipped} 条重复已跳过")
        }
    },
    bib_parse_error: |line| format!("第 {line} 行附近的 BibTeX 条目无法解析，已跳过"),
    no_bib_entries: "没有找到 BibTeX 条目",
    export_bibtex: "导出 BibTeX",
    export_selected: "导出所选 BibTeX",

    col_title: "标题",
    col_authors: "作者",
    col_year: "年份",
    col_venue: "期刊 / 会议",
    col_type: "类型",
    col_key: "引用 key",
    col_added: "添加时间",
    col_deleted: "删除时间",
    resize_column: "拖动调整列宽，双击还原",
    untitled: "（无标题）",
    empty_title: "这里还没有条目",
    empty_hint: "在上方粘贴 DOI、doi.org / ACM / Springer 链接、arXiv 编号或 BibTeX 即可添加，也可以导入 .bib 文件。",
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
    field_title: "标题（{} 中的大小写导出时保持不变）",
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
    in_subcollection: "通过子目录包含；取消勾选会从这个目录及其所有子目录中移出",
    bibtex_preview: "BibTeX 预览",
    save: "保存",
    saved: "已保存",

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

    settings: "设置",
    appearance: "外观",
    language: "语言",
    theme_name: |t| match t {
        Theme::System => "跟随系统",
        Theme::Light => "浅色",
        Theme::Dark => "深色",
    },

    install_section: "安装为应用",
    install_button: "安装",
    install_hint: "安装后可以像普通应用一样从 Dock / 开始菜单打开，离线也能使用。Chrome / Edge 也可以点地址栏里的安装图标；Safari 请用「文件 → 添加到程序坞」，iPhone / iPad 用「分享 → 添加到主屏幕」。",
    installed: "已作为应用安装。",

    sync_section: "云同步（GitHub Gist）",
    sync_intro: "文献库默认保存在当前浏览器中。要在多台设备上使用，可以同步到你 GitHub 账号下的私密（secret）Gist。",
    field_token: "GitHub token",
    token_hint: "创建一个带 gist 权限的 token 粘贴到这里，第一次同步会自动创建 Gist；其他设备用同一账号的 token 会自动找到它。Token 只保存在本设备上。",
    create_token: "创建 token ↗",
    open_gist: "打开 Gist ↗",
    open_link: "在新标签页打开",
    auto_sync: "修改后自动同步",
    save_and_sync: "保存并同步",
    disconnect: "断开",
    sync_now: "立即同步",
    syncing: "同步中…",
    sync_done: "同步完成",
    synced_at: |time| format!("已同步 {time}"),
    unsynced: "有未同步的更改",
    sync_failed: "同步失败",
    sync_error: |e| match e {
        SyncError::NotConfigured => "请先在设置中填写 GitHub token".into(),
        SyncError::Unauthorized => "同步失败：token 无效或已过期".into(),
        SyncError::Forbidden(m) => format!("同步失败：token 没有 gist 权限。{m}"),
        SyncError::Network(m) => format!("同步失败：网络错误 {m}"),
        SyncError::Http(code, m) => format!("同步失败：GitHub 返回 HTTP {code} {m}"),
        SyncError::InvalidFile(m) => format!("同步失败：远端文件不是有效的 easy-ref 文献库（{m}）"),
    },
};

pub const EN: Strings = Strings {
    library: "Library",
    trash: "Trash",
    trash_empty: "Trash is empty",
    restore: "Restore",
    select_to_restore: "Select entries to restore them, or drag them back onto Library or a collection",
    delete_forever: "Delete permanently",
    empty_trash: "Empty trash",
    confirm_empty_trash: |n| format!("Permanently delete the {n} entries in the trash? This cannot be undone."),
    moved_to_trash: |n| format!("Moved {n} to the trash"),
    restored: |n| format!("Restored {n}"),
    replaced: |old, new| {
        if old == new {
            format!("Updated {old} to the newer version")
        } else {
            format!("Updated {old} to the newer version {new}")
        }
    },
    collections: "Collections",
    new_collection: "New collection",
    new_subcollection: "New subcollection",
    collection_name: "Collection name",
    collections_hint: "Click + to create a collection, then drag entries onto it.",
    rename_hint: "Double-click to rename; drop on another collection's edge to reorder, on its middle to nest",
    rename: "Rename",
    resize_sidebar: "Drag to resize, double-click to reset",
    delete_collection: "Delete collection",
    confirm_delete_collection: |name, subfolders| {
        if subfolders == 0 {
            format!("Delete collection \"{name}\"? Its entries stay in the library.")
        } else {
            format!(
                "Delete collection \"{name}\" and its {subfolders} subcollection(s)? The entries stay in the library."
            )
        }
    },

    add_placeholder: "Paste DOIs, links, arXiv IDs or BibTeX (several at once), press Enter",
    add: "Add",
    import: "Import",
    bib_detected: "BibTeX detected: Enter adds a line, ⌘/Ctrl + Enter imports",
    looking_up: |n| format!("Looking up {n}…"),
    unrecognized: |raw| {
        format!("Not recognized: {raw} (for IEEE Xplore and similar links, copy the DOI from the page)")
    },
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
    confirm_delete_entries: |n| format!("Permanently delete {n} entries? This cannot be undone."),
    new_entry: "New entry",
    import_bib: "Import .bib",
    imported: |added, skipped| {
        let noun = if added == 1 { "entry" } else { "entries" };
        if skipped == 0 {
            format!("Imported {added} {noun}")
        } else {
            format!("Imported {added} {noun}, skipped {skipped} duplicate(s)")
        }
    },
    bib_parse_error: |line| format!("Skipped a BibTeX entry near line {line} that could not be parsed"),
    no_bib_entries: "No BibTeX entries found",
    export_bibtex: "Export BibTeX",
    export_selected: "Export selected",

    col_title: "Title",
    col_authors: "Authors",
    col_year: "Year",
    col_venue: "Venue",
    col_type: "Type",
    col_key: "Key",
    col_added: "Added",
    col_deleted: "Deleted",
    resize_column: "Drag to resize, double-click to reset",
    untitled: "(untitled)",
    empty_title: "No entries yet",
    empty_hint: "Paste a DOI, a doi.org / ACM / Springer link, an arXiv ID or BibTeX above, or import a .bib file.",
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
    field_title: "Title (capitalization inside {} is kept on export)",
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
    in_subcollection: "Included through a subcollection; unticking removes it from this collection and all below",
    bibtex_preview: "BibTeX preview",
    save: "Save",
    saved: "Saved",

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

    settings: "Settings",
    appearance: "Appearance",
    language: "Language",
    theme_name: |t| match t {
        Theme::System => "System",
        Theme::Light => "Light",
        Theme::Dark => "Dark",
    },

    install_section: "Install app",
    install_button: "Install",
    install_hint: "Once installed it opens from the Dock or Start menu like a regular app and works offline. In Chrome or Edge you can also use the install icon in the address bar; in Safari use File → Add to Dock, and on iPhone or iPad use Share → Add to Home Screen.",
    installed: "Installed as an app.",

    sync_section: "Sync (GitHub Gist)",
    sync_intro: "The library is stored in this browser. To use it on several devices, sync it to a secret gist in your GitHub account.",
    field_token: "GitHub token",
    token_hint: "Paste a token with the gist permission. The first sync creates the gist; other devices using a token from the same account find it automatically. The token is stored only on this device.",
    create_token: "Create a token ↗",
    open_gist: "Open gist ↗",
    open_link: "Open in a new tab",
    auto_sync: "Sync automatically after changes",
    save_and_sync: "Save and sync",
    disconnect: "Disconnect",
    sync_now: "Sync now",
    syncing: "Syncing…",
    sync_done: "Sync complete",
    synced_at: |time| format!("Synced {time}"),
    unsynced: "Unsynced changes",
    sync_failed: "Sync failed",
    sync_error: |e| match e {
        SyncError::NotConfigured => "Set a GitHub token in Settings first".into(),
        SyncError::Unauthorized => "Sync failed: the token is invalid or expired".into(),
        SyncError::Forbidden(m) => format!("Sync failed: the token lacks the gist permission. {m}"),
        SyncError::Network(m) => format!("Sync failed: network error {m}"),
        SyncError::Http(code, m) => format!("Sync failed: GitHub returned HTTP {code} {m}"),
        SyncError::InvalidFile(m) => format!("Sync failed: the remote file is not a valid easy-ref library ({m})"),
    },
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
        let p = Problem::Missing {
            key: "k".into(),
            fields: vec!["author", "year"],
        };
        assert_eq!((EN.problem)(&p), "k: missing author, year");
    }
}
