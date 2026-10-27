//! Syncs the library with a secret GitHub gist.

use crate::model::{Collection, Entry, Library, disambiguate_key, normalize_doi};
use gloo_net::http::{Request, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

const API: &str = "https://api.github.com";
pub const FILE: &str = "easy-ref-library.json";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub token: String,
    /// Found or created on the first sync.
    pub gist_id: String,
    pub auto: bool,
}

impl SyncConfig {
    pub fn is_configured(&self) -> bool {
        !self.token.trim().is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SyncError {
    NotConfigured,
    Unauthorized,
    Forbidden(String),
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
    // Live copies before trashed ones, then oldest first.
    entries.sort_by(|x, y| {
        (x.trashed.is_some(), x.added, &x.id)
            .partial_cmp(&(y.trashed.is_some(), y.added, &y.id))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

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
    entries.sort_by(|x, y| x.added.total_cmp(&y.added).then_with(|| x.id.cmp(&y.id)));

    // Live entries keep their keys before trashed ones.
    let mut keys: HashSet<String> = HashSet::new();
    for trashed in [false, true] {
        for e in entries.iter_mut().filter(|e| e.trashed.is_some() == trashed) {
            e.key = disambiguate_key(&e.key, &e.title, |k| keys.contains(k));
            keys.insert(e.key.clone());
        }
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

/// Pulls the gist, merges and pushes. Returns the merged library and the gist id.
pub async fn sync(config: &SyncConfig, local: &Library) -> Result<(Library, String), SyncError> {
    let token = config.token.trim();
    if token.is_empty() {
        return Err(SyncError::NotConfigured);
    }
    let mut id = config.gist_id.trim().to_string();
    let mut remote = None;
    if !id.is_empty() {
        match read_gist(token, &id).await {
            Ok(r) => remote = r,
            Err(SyncError::Http(404, _)) => id.clear(),
            Err(e) => return Err(e),
        }
    }
    if id.is_empty()
        && let Some(found) = find_gist(token).await?
    {
        remote = read_gist(token, &found).await?;
        id = found;
    }

    let merged = match &remote {
        Some(r) => merge(local, r),
        None => merge(local, local),
    };
    if remote.as_ref().is_some_and(|r| merge(r, r) == merged) {
        return Ok((merged, id));
    }
    let content = serde_json::to_string_pretty(&merged).map_err(|e| SyncError::InvalidFile(e.to_string()))?;
    let files = json!({ FILE: { "content": content } });
    if id.is_empty() {
        let body = json!({ "description": "easy-ref library", "public": false, "files": files });
        let created = send(authorized(Request::post(&format!("{API}/gists")), token).json(&body)).await?;
        id = created["id"].as_str().unwrap_or_default().to_string();
    } else {
        let body = json!({ "files": files });
        send(authorized(Request::patch(&format!("{API}/gists/{id}")), token).json(&body)).await?;
    }
    Ok((merged, id))
}

fn authorized(builder: RequestBuilder, token: &str) -> RequestBuilder {
    builder
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .cache(web_sys::RequestCache::NoStore)
}

async fn send(request: Result<Request, gloo_net::Error>) -> Result<Value, SyncError> {
    let resp = request
        .map_err(|e| SyncError::Network(e.to_string()))?
        .send()
        .await
        .map_err(|e| SyncError::Network(e.to_string()))?;
    let status = resp.status();
    if (200..300).contains(&status) {
        return resp.json().await.map_err(|e| SyncError::InvalidFile(e.to_string()));
    }
    let message = resp
        .json::<Value>()
        .await
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_string))
        .unwrap_or_default();
    Err(match status {
        401 => SyncError::Unauthorized,
        403 => SyncError::Forbidden(message),
        code => SyncError::Http(code, message),
    })
}

/// Id of a gist of this account that contains [`FILE`].
async fn find_gist(token: &str) -> Result<Option<String>, SyncError> {
    for page in 1..=10 {
        let list =
            send(authorized(Request::get(&format!("{API}/gists?per_page=100&page={page}")), token).build()).await?;
        let Some(gists) = list.as_array() else { break };
        if let Some(gist) = gists.iter().find(|g| g["files"].get(FILE).is_some()) {
            return Ok(gist["id"].as_str().map(str::to_string));
        }
        if gists.len() < 100 {
            break;
        }
    }
    Ok(None)
}

async fn read_gist(token: &str, id: &str) -> Result<Option<Library>, SyncError> {
    let gist = send(authorized(Request::get(&format!("{API}/gists/{id}")), token).build()).await?;
    let file = &gist["files"][FILE];
    if file.is_null() {
        return Ok(None);
    }
    let content = if file["truncated"].as_bool() == Some(true) {
        let url = file["raw_url"].as_str().unwrap_or_default();
        let resp = Request::get(url)
            .send()
            .await
            .map_err(|e| SyncError::Network(e.to_string()))?;
        resp.text().await.map_err(|e| SyncError::Network(e.to_string()))?
    } else {
        file["content"].as_str().unwrap_or_default().to_string()
    };
    if content.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|e| SyncError::InvalidFile(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Person, Place};

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
            a.create_collection(id.into(), id, None, 1.0);
        }
        let b = a.clone();
        a.move_collection("c3", "c1", Place::Before, 2.0);
        a.move_collection("c2", "c1", Place::Inside, 3.0);
        let merged = merge(&b, &a);
        let tree: Vec<_> = merged
            .tree()
            .into_iter()
            .map(|(c, depth)| (c.id.as_str(), depth))
            .collect();
        assert_eq!(tree, [("c3", 0), ("c1", 0), ("c2", 1)]);
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
    fn live_entry_keeps_key_over_trashed_entry() {
        let mut a = Library::default();
        let mut old = entry("old", "10.1/a", "He2016CVPR", 1.0, 2.0);
        old.trashed = Some(2.0);
        a.entries.push(old);
        a.entries.push(entry("new", "10.1/b", "He2016CVPR", 3.0, 3.0));
        let merged = merge(&a, &a);
        let key = |id: &str| merged.entry(id).unwrap().key.clone();
        assert_eq!(
            (key("new"), key("old")),
            ("He2016CVPR".to_string(), "He2016CVPRTitle".to_string())
        );
    }

    #[test]
    fn live_copy_wins_over_trashed_copy_of_same_doi() {
        let mut a = Library::default();
        let mut old = entry("old", "10.1/x", "K", 1.0, 2.0);
        old.trashed = Some(2.0);
        a.entries.push(old);
        let mut b = Library::default();
        b.entries.push(entry("new", "10.1/X", "K", 3.0, 3.0));
        let merged = merge(&a, &b);
        assert_eq!(
            merged.entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["new"]
        );
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
        assert_eq!(keys, ["He2016CVPR", "He2016CVPRTitle"]);
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
        assert!(!SyncConfig::default().is_configured());
        assert!(
            SyncConfig {
                token: "ghp_x".into(),
                ..Default::default()
            }
            .is_configured()
        );
        let old: SyncConfig =
            serde_json::from_str(r#"{"token":"t","repo":"me/refs","branch":"main","auto":true}"#).unwrap();
        assert_eq!((old.token.as_str(), old.gist_id.as_str(), old.auto), ("t", "", true));
    }
}
