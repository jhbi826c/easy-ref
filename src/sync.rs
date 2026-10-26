//! Syncs the library with a JSON file in a GitHub repository.

use crate::model::{Collection, Entry, Library, alpha_suffix, normalize_doi};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use gloo_net::http::{Request, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub token: String,
    /// `owner/repo`
    pub repo: String,
    pub branch: String,
    pub path: String,
    pub auto: bool,
}

pub const DEFAULT_BRANCH: &str = "main";
pub const DEFAULT_PATH: &str = "library.json";

impl SyncConfig {
    pub fn is_configured(&self) -> bool {
        let parts: Vec<&str> = self.repo.trim().split('/').collect();
        !self.token.trim().is_empty() && parts.len() == 2 && parts.iter().all(|p| !p.is_empty())
    }

    fn branch(&self) -> &str {
        let b = self.branch.trim();
        if b.is_empty() { DEFAULT_BRANCH } else { b }
    }

    fn path(&self) -> &str {
        let p = self.path.trim().trim_matches('/');
        if p.is_empty() { DEFAULT_PATH } else { p }
    }

    fn url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/contents/{}",
            self.repo.trim(),
            self.path()
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SyncError {
    NotConfigured,
    Unauthorized,
    Forbidden(String),
    NotFound,
    Conflict,
    Network(String),
    Http(u16, String),
    InvalidFile(String),
}

/// Merges two replicas. Newer edits win, deletions are kept as tombstones,
/// and copies of the same DOI added on different devices collapse into one.
pub fn merge(a: &Library, b: &Library) -> Library {
    let mut deleted: BTreeMap<String, f64> = a.deleted.clone();
    for (id, &t) in &b.deleted {
        let slot = deleted.entry(id.clone()).or_insert(t);
        *slot = slot.max(t);
    }
    let alive =
        |deleted: &BTreeMap<String, f64>, id: &str, modified: f64| deleted.get(id).is_none_or(|&t| t < modified);

    let mut by_id: HashMap<String, Entry> = HashMap::new();
    for e in a.entries.iter().chain(&b.entries) {
        if by_id.get(&e.id).is_none_or(|cur| e.modified > cur.modified) {
            by_id.insert(e.id.clone(), e.clone());
        }
    }
    let mut entries: Vec<Entry> = by_id
        .into_values()
        .filter(|e| alive(&deleted, &e.id, e.modified))
        .collect();
    entries.sort_by(|x, y| x.added.total_cmp(&y.added).then_with(|| x.id.cmp(&y.id)));

    let mut first_with_doi: HashMap<String, String> = HashMap::new();
    let mut replaced: HashMap<String, String> = HashMap::new();
    entries.retain(|e| {
        let doi = normalize_doi(&e.doi);
        if doi.is_empty() {
            return true;
        }
        match first_with_doi.get(&doi) {
            Some(keep) => {
                replaced.insert(e.id.clone(), keep.clone());
                deleted.insert(e.id.clone(), e.modified);
                false
            }
            None => {
                first_with_doi.insert(doi, e.id.clone());
                true
            }
        }
    });

    let mut keys: HashSet<String> = HashSet::new();
    for e in &mut entries {
        if keys.contains(&e.key) {
            let base = e.key.clone();
            e.key = (0..)
                .map(|n| format!("{base}{}", alpha_suffix(n)))
                .find(|k| !keys.contains(k))
                .unwrap();
        }
        keys.insert(e.key.clone());
    }

    let mut by_id: HashMap<String, Collection> = HashMap::new();
    for c in a.collections.iter().chain(&b.collections) {
        if by_id.get(&c.id).is_none_or(|cur| c.modified > cur.modified) {
            by_id.insert(c.id.clone(), c.clone());
        }
    }
    let live: HashSet<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let mut collections: Vec<Collection> = by_id
        .into_values()
        .filter(|c| alive(&deleted, &c.id, c.modified))
        .map(|mut c| {
            let mut seen = HashSet::new();
            c.entries = c
                .entries
                .iter()
                .map(|id| replaced.get(id).unwrap_or(id).clone())
                .filter(|id| live.contains(id.as_str()) && seen.insert(id.clone()))
                .collect();
            c
        })
        .collect();
    collections.sort_by(|x, y| x.order.total_cmp(&y.order).then_with(|| x.id.cmp(&y.id)));

    Library {
        entries,
        collections,
        deleted,
    }
}

/// Pulls, merges and pushes. Returns the merged library now stored remotely.
pub async fn sync(config: &SyncConfig, local: &Library) -> Result<Library, SyncError> {
    if !config.is_configured() {
        return Err(SyncError::NotConfigured);
    }
    for _ in 0..3 {
        let remote = fetch(config).await?;
        let (merged, sha) = match &remote {
            Some((lib, sha)) => {
                let merged = merge(local, lib);
                if merged == merge(lib, lib) {
                    return Ok(merged);
                }
                (merged, Some(sha.as_str()))
            }
            None => (merge(local, local), None),
        };
        match push(config, &merged, sha).await {
            Ok(()) => return Ok(merged),
            Err(SyncError::Conflict) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(SyncError::Conflict)
}

#[derive(Deserialize)]
struct ContentFile {
    sha: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    encoding: String,
}

fn authorized(builder: RequestBuilder, config: &SyncConfig, accept: &str) -> RequestBuilder {
    builder
        .header("Authorization", &format!("Bearer {}", config.token.trim()))
        .header("Accept", accept)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .cache(web_sys::RequestCache::NoStore)
}

async fn error_for(resp: Response) -> SyncError {
    let status = resp.status();
    let message = resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_string))
        .unwrap_or_default();
    match status {
        401 => SyncError::Unauthorized,
        403 => SyncError::Forbidden(message),
        404 => SyncError::NotFound,
        409 => SyncError::Conflict,
        422 if message.contains("sha") => SyncError::Conflict,
        code => SyncError::Http(code, message),
    }
}

fn network(e: gloo_net::Error) -> SyncError {
    SyncError::Network(e.to_string())
}

/// The remote library and its blob sha, or `None` if the file does not exist yet.
async fn fetch(config: &SyncConfig) -> Result<Option<(Library, String)>, SyncError> {
    let url = format!("{}?ref={}", config.url(), config.branch());
    let resp = authorized(Request::get(&url), config, "application/vnd.github+json")
        .send()
        .await
        .map_err(network)?;
    match resp.status() {
        200 => {}
        404 => return Ok(None),
        _ => return Err(error_for(resp).await),
    }
    let file: ContentFile = resp.json().await.map_err(|e| SyncError::InvalidFile(e.to_string()))?;
    let text = if file.encoding == "base64" {
        let raw: String = file.content.chars().filter(|c| !c.is_whitespace()).collect();
        let bytes = BASE64.decode(raw).map_err(|e| SyncError::InvalidFile(e.to_string()))?;
        String::from_utf8(bytes).map_err(|e| SyncError::InvalidFile(e.to_string()))?
    } else {
        // Files over 1 MB come without inline content.
        let resp = authorized(Request::get(&url), config, "application/vnd.github.raw+json")
            .send()
            .await
            .map_err(network)?;
        if resp.status() != 200 {
            return Err(error_for(resp).await);
        }
        resp.text().await.map_err(network)?
    };
    let lib = serde_json::from_str(&text).map_err(|e| SyncError::InvalidFile(e.to_string()))?;
    Ok(Some((lib, file.sha)))
}

async fn push(config: &SyncConfig, lib: &Library, sha: Option<&str>) -> Result<(), SyncError> {
    let json = serde_json::to_string_pretty(lib).map_err(|e| SyncError::InvalidFile(e.to_string()))?;
    let mut body = serde_json::json!({
        "message": "Update library",
        "content": BASE64.encode(json),
        "branch": config.branch(),
    });
    if let Some(sha) = sha {
        body["sha"] = sha.into();
    }
    let resp = authorized(Request::put(&config.url()), config, "application/vnd.github+json")
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(network)?
        .send()
        .await
        .map_err(network)?;
    match resp.status() {
        200 | 201 => Ok(()),
        _ => Err(error_for(resp).await),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Person;

    fn entry(id: &str, doi: &str, key: &str, added: f64, modified: f64) -> Entry {
        Entry {
            id: id.into(),
            doi: doi.into(),
            key: key.into(),
            title: format!("Title {id}"),
            authors: vec![Person {
                family: "A".into(),
                ..Default::default()
            }],
            added,
            modified,
            ..Default::default()
        }
    }

    fn collection(id: &str, entries: &[&str], modified: f64) -> Collection {
        Collection {
            id: id.into(),
            name: id.into(),
            entries: entries.iter().map(|s| s.to_string()).collect(),
            modified,
            ..Default::default()
        }
    }

    #[test]
    fn collection_order_propagates() {
        let mut a = Library::default();
        for id in ["c1", "c2", "c3"] {
            a.create_collection(id.into(), id, 1.0);
        }
        let b = a.clone();
        a.move_collection("c3", "c1", false, 2.0);
        let names: Vec<_> = merge(&b, &a).collections.into_iter().map(|c| c.id).collect();
        assert_eq!(names, ["c3", "c1", "c2"]);
    }

    #[test]
    fn newer_edit_wins_both_ways() {
        let mut a = Library::default();
        a.entries.push(entry("1", "", "k", 1.0, 5.0));
        let mut b = a.clone();
        b.entries[0].title = "edited".into();
        b.entries[0].modified = 9.0;
        assert_eq!(merge(&a, &b).entries[0].title, "edited");
        assert_eq!(merge(&b, &a).entries[0].title, "edited");
        assert_eq!(merge(&a, &b), merge(&b, &a));
    }

    #[test]
    fn deletions_propagate_unless_edited_later() {
        let mut a = Library::default();
        a.entries.push(entry("1", "", "k", 1.0, 1.0));
        a.collections.push(collection("c", &["1"], 1.0));
        let b = a.clone();
        a.delete_entries(&["1".into()], 2.0);
        let merged = merge(&a, &b);
        assert!(merged.entries.is_empty());
        assert!(merged.collections[0].entries.is_empty());

        let mut b = b;
        b.entries[0].modified = 3.0;
        assert_eq!(merge(&a, &b).entries.len(), 1);
    }

    #[test]
    fn same_doi_from_two_devices_collapses() {
        let mut a = Library::default();
        a.entries.push(entry("x", "10.1/ABC", "He2016CVPR", 1.0, 1.0));
        a.collections.push(collection("c1", &["x"], 1.0));
        let mut b = Library::default();
        b.entries.push(entry("y", "10.1/abc", "He2016CVPR", 2.0, 2.0));
        b.collections.push(collection("c2", &["y"], 2.0));
        let merged = merge(&a, &b);
        assert_eq!(merged.entries.len(), 1);
        assert_eq!(merged.entries[0].id, "x");
        assert_eq!(merged.collections[1].entries, ["x"]);
        assert!(merged.deleted.contains_key("y"));
        assert_eq!(merge(&merged, &b), merged);
    }

    #[test]
    fn clashing_keys_are_suffixed() {
        let mut a = Library::default();
        a.entries.push(entry("x", "10.1/a", "He2016CVPR", 1.0, 1.0));
        let mut b = Library::default();
        b.entries.push(entry("y", "10.1/b", "He2016CVPR", 2.0, 2.0));
        let keys: Vec<_> = merge(&a, &b).entries.into_iter().map(|e| e.key).collect();
        assert_eq!(keys, ["He2016CVPR", "He2016CVPRa"]);
    }

    #[test]
    fn merge_is_idempotent() {
        let mut a = Library::default();
        a.entries.push(entry("2", "", "b", 2.0, 2.0));
        a.entries.push(entry("1", "", "a", 1.0, 1.0));
        a.collections.push(collection("c", &["2", "1", "2"], 1.0));
        let once = merge(&a, &a);
        assert_eq!(merge(&once, &once), once);
        assert_eq!(once.collections[0].entries, ["2", "1"]);
    }

    #[test]
    fn config_validation() {
        let mut c = SyncConfig {
            token: "t".into(),
            repo: "me/refs".into(),
            ..Default::default()
        };
        assert!(c.is_configured());
        assert_eq!(c.url(), "https://api.github.com/repos/me/refs/contents/library.json");
        c.repo = "refs".into();
        assert!(!c.is_configured());
    }
}
