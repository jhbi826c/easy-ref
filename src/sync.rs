//! Syncs the library with a private GitHub repository, split into small files.

use crate::model::{Collection, Entry, Library, disambiguate_key, normalize_doi};
use gloo_net::http::{Request, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

const API: &str = "https://api.github.com";
pub const DEFAULT_REPO: &str = "easy-ref-library";
/// Number of files the entries are spread over.
const SHARDS: u32 = 16;
const COLLECTIONS: &str = "collections.json";
const DELETED: &str = "deleted.json";
const README: &str = "# easy-ref library\n\nSynced by easy-ref. Entries are in `entries/`, collections in \
                      `collections.json` and deleted ids in `deleted.json`.\n";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    pub token: String,
    /// `name` or `owner/name`; empty means [`DEFAULT_REPO`] in the token's account.
    pub repo: String,
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
    /// Repository that is missing or not accessible with the token.
    NoRepo(String),
    /// Other devices kept pushing at the same time.
    Conflict,
    Network(String),
    Http(u16, String),
    InvalidFile(String),
}

/// File that holds an entry, e.g. `entries/7.json`.
fn shard(id: &str) -> String {
    let hash = id
        .bytes()
        .fold(0x811c_9dc5_u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    format!("entries/{:x}.json", hash % SHARDS)
}

fn is_data_file(path: &str) -> bool {
    path == COLLECTIONS || path == DELETED || (path.starts_with("entries/") && path.ends_with(".json"))
}

/// A JSON array with one item per line.
fn json_lines<T: Serialize>(items: &[&T]) -> String {
    let lines: Vec<String> = items.iter().filter_map(|i| serde_json::to_string(i).ok()).collect();
    if lines.is_empty() {
        "[]\n".into()
    } else {
        format!("[\n{}\n]\n", lines.join(",\n"))
    }
}

/// The library as repository files, path → content. Empty entry files are left out.
pub fn to_files(lib: &Library) -> BTreeMap<String, String> {
    let mut shards: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
    for e in &lib.entries {
        shards.entry(shard(&e.id)).or_default().push(e);
    }
    let mut files: BTreeMap<String, String> = shards
        .into_iter()
        .map(|(path, mut entries)| {
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            (path, json_lines(&entries))
        })
        .collect();
    let mut collections: Vec<&Collection> = lib.collections.iter().collect();
    collections.sort_by(|a, b| a.id.cmp(&b.id));
    files.insert(COLLECTIONS.into(), json_lines(&collections));
    let deleted = serde_json::to_string_pretty(&lib.deleted).unwrap_or_default();
    files.insert(DELETED.into(), deleted + "\n");
    files
}

/// Adds one repository file to a library.
fn read_file(lib: &mut Library, path: &str, content: &str) -> Result<(), SyncError> {
    let invalid = |e: serde_json::Error| SyncError::InvalidFile(format!("{path}: {e}"));
    match path {
        COLLECTIONS => lib
            .collections
            .extend(serde_json::from_str::<Vec<Collection>>(content).map_err(invalid)?),
        DELETED => lib
            .deleted
            .extend(serde_json::from_str::<BTreeMap<String, f64>>(content).map_err(invalid)?),
        _ => lib
            .entries
            .extend(serde_json::from_str::<Vec<Entry>>(content).map_err(invalid)?),
    }
    Ok(())
}

/// Git blob id of a file.
fn blob_sha(content: &str) -> String {
    let mut hash = sha1_smol::Sha1::new();
    hash.update(format!("blob {}\0", content.len()).as_bytes());
    hash.update(content.as_bytes());
    hash.digest().to_string()
}

/// Remote data files that differ from the local ones. `remote` maps path → blob id.
fn to_download(local: &BTreeMap<String, String>, remote: &BTreeMap<String, String>) -> Vec<String> {
    remote
        .iter()
        .filter(|(path, sha)| is_data_file(path) && local.get(*path).is_none_or(|c| blob_sha(c) != **sha))
        .map(|(path, _)| path.clone())
        .collect()
}

/// Writes and removals (`None`) that turn the remote files into `files`.
fn changes(files: &BTreeMap<String, String>, remote: &BTreeMap<String, String>) -> Vec<(String, Option<String>)> {
    let mut changes: Vec<(String, Option<String>)> = files
        .iter()
        .filter(|(path, content)| remote.get(*path) != Some(&blob_sha(content)))
        .map(|(path, content)| (path.clone(), Some(content.clone())))
        .collect();
    changes.extend(
        remote
            .keys()
            .filter(|path| is_data_file(path) && !files.contains_key(*path))
            .map(|path| (path.clone(), None)),
    );
    changes
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

/// Pulls the files that changed, merges and pushes the result in one commit.
/// Returns the merged library and the repository as `owner/name`.
pub async fn sync(config: &SyncConfig, local: &Library) -> Result<(Library, String), SyncError> {
    let token = config.token.trim();
    if token.is_empty() {
        return Err(SyncError::NotConfigured);
    }
    let repo = open_repo(token, &config.repo).await?;
    let mut local = local.clone();
    for _ in 0..4 {
        let Some((parent, tree)) = head(token, &repo).await? else {
            init(token, &repo).await?;
            continue;
        };
        let remote = remote_files(token, &repo, &tree).await?;
        let mut pulled = Library::default();
        for path in to_download(&to_files(&local), &remote) {
            read_file(&mut pulled, &path, &blob(token, &repo, &remote[&path]).await?)?;
        }
        let merged = merge(&local, &pulled);
        let changes = changes(&to_files(&merged), &remote);
        if changes.is_empty() || commit(token, &repo, &parent, &tree, &changes).await? {
            return Ok((merged, repo.name));
        }
        local = merged;
    }
    Err(SyncError::Conflict)
}

struct Repo {
    /// `owner/name`
    name: String,
    branch: String,
}

fn api(method: fn(&str) -> RequestBuilder, path: &str, token: &str) -> RequestBuilder {
    method(&format!("{API}{path}"))
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .cache(web_sys::RequestCache::NoStore)
}

fn network(e: gloo_net::Error) -> SyncError {
    SyncError::Network(e.to_string())
}

async fn fetch(request: Result<Request, gloo_net::Error>) -> Result<Response, SyncError> {
    let resp = request.map_err(network)?.send().await.map_err(network)?;
    let status = resp.status();
    if (200..300).contains(&status) {
        return Ok(resp);
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

async fn send(request: Result<Request, gloo_net::Error>) -> Result<Value, SyncError> {
    fetch(request)
        .await?
        .json()
        .await
        .map_err(|e| SyncError::InvalidFile(e.to_string()))
}

async fn get(token: &str, path: &str) -> Result<Value, SyncError> {
    send(api(Request::get, path, token).build()).await
}

async fn post(token: &str, path: &str, body: &Value) -> Result<Value, SyncError> {
    send(api(Request::post, path, token).json(body)).await
}

async fn login(token: &str) -> Result<String, SyncError> {
    let user = get(token, "/user").await?;
    Ok(user["login"].as_str().unwrap_or_default().to_string())
}

/// Finds the repository, creating it as a private one in the token's account if missing.
async fn open_repo(token: &str, repo: &str) -> Result<Repo, SyncError> {
    let repo = repo.trim().trim_matches('/');
    let mut user = None;
    let (owner, name) = match repo.split_once('/') {
        Some((owner, name)) => (owner.to_string(), name.to_string()),
        None => {
            let login = login(token).await?;
            user = Some(login.clone());
            (login, if repo.is_empty() { DEFAULT_REPO } else { repo }.to_string())
        }
    };
    let missing = || SyncError::NoRepo(format!("{owner}/{name}"));
    let info = match get(token, &format!("/repos/{owner}/{name}")).await {
        Err(SyncError::Http(404, _)) => {
            let user = match user {
                Some(user) => user,
                None => login(token).await?,
            };
            if !user.eq_ignore_ascii_case(&owner) {
                return Err(missing());
            }
            let body = json!({ "name": name, "private": true, "auto_init": true, "description": "easy-ref library" });
            match post(token, "/user/repos", &body).await {
                // The name is taken by a repository the token cannot see.
                Err(SyncError::Http(422, _)) => return Err(missing()),
                // Tokens without the repo scope get 404.
                Err(SyncError::Http(404, m)) => return Err(SyncError::Forbidden(m)),
                other => other?,
            }
        }
        other => other?,
    };
    Ok(Repo {
        name: info["full_name"]
            .as_str()
            .map_or_else(|| format!("{owner}/{name}"), str::to_string),
        branch: info["default_branch"].as_str().unwrap_or("main").to_string(),
    })
}

/// Head commit and its tree, or `None` for an empty repository.
async fn head(token: &str, repo: &Repo) -> Result<Option<(String, String)>, SyncError> {
    match get(token, &format!("/repos/{}/branches/{}", repo.name, repo.branch)).await {
        Ok(branch) => {
            let commit = &branch["commit"];
            let sha = |v: &Value| v.as_str().unwrap_or_default().to_string();
            Ok(Some((sha(&commit["sha"]), sha(&commit["commit"]["tree"]["sha"]))))
        }
        Err(SyncError::Http(404 | 409, _)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Adds a README so the Git Data API works on an empty repository.
async fn init(token: &str, repo: &Repo) -> Result<(), SyncError> {
    let content = web_sys::window().and_then(|w| w.btoa(README).ok()).unwrap_or_default();
    let body = json!({ "message": "Create easy-ref library", "content": content });
    let path = format!("/repos/{}/contents/README.md", repo.name);
    send(api(Request::put, &path, token).json(&body)).await.map(|_| ())
}

/// Blob ids of all files in a tree, path → sha.
async fn remote_files(token: &str, repo: &Repo, tree: &str) -> Result<BTreeMap<String, String>, SyncError> {
    let tree = get(token, &format!("/repos/{}/git/trees/{tree}?recursive=1", repo.name)).await?;
    Ok(tree["tree"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["type"] == "blob")
        .filter_map(|f| Some((f["path"].as_str()?.to_string(), f["sha"].as_str()?.to_string())))
        .collect())
}

async fn blob(token: &str, repo: &Repo, sha: &str) -> Result<String, SyncError> {
    let request = api(Request::get, &format!("/repos/{}/git/blobs/{sha}", repo.name), token)
        .header("Accept", "application/vnd.github.raw+json")
        .build();
    fetch(request).await?.text().await.map_err(network)
}

/// Commits the changes on top of `parent`. `false` if the branch moved meanwhile.
async fn commit(
    token: &str,
    repo: &Repo,
    parent: &str,
    base_tree: &str,
    changes: &[(String, Option<String>)],
) -> Result<bool, SyncError> {
    let items: Vec<Value> = changes
        .iter()
        .map(|(path, content)| match content {
            Some(content) => json!({ "path": path, "mode": "100644", "type": "blob", "content": content }),
            None => json!({ "path": path, "mode": "100644", "type": "blob", "sha": null }),
        })
        .collect();
    let path = format!("/repos/{}/git/trees", repo.name);
    let tree = post(token, &path, &json!({ "base_tree": base_tree, "tree": items })).await?;
    let tree = tree["sha"].as_str().unwrap_or_default();
    if tree == base_tree {
        return Ok(true);
    }
    let path = format!("/repos/{}/git/commits", repo.name);
    let body = json!({ "message": "Sync library", "tree": tree, "parents": [parent] });
    let commit = post(token, &path, &body).await?;
    let body = json!({ "sha": commit["sha"], "force": false });
    let path = format!("/repos/{}/git/refs/heads/{}", repo.name, repo.branch);
    match send(api(Request::patch, &path, token).json(&body)).await {
        Ok(_) => Ok(true),
        Err(SyncError::Http(409 | 422, _)) => Ok(false),
        Err(e) => Err(e),
    }
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
        let old: SyncConfig = serde_json::from_str(r#"{"token":"t","gist_id":"abc","auto":true}"#).unwrap();
        assert_eq!((old.token.as_str(), old.repo.as_str(), old.auto), ("t", "", true));
    }

    fn library(n: usize) -> Library {
        let mut lib = Library::default();
        for i in 0..n {
            lib.entries.push(entry(
                &format!("e{i}"),
                &format!("10.1/{i}"),
                &format!("K{i}"),
                i as f64,
                i as f64,
            ));
        }
        lib.collections.push(collection("c", &["e1", "e2"], 1.0));
        lib.deleted.insert("gone".into(), 5.0);
        lib
    }

    fn shas(files: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        files.iter().map(|(p, c)| (p.clone(), blob_sha(c))).collect()
    }

    fn read_all(files: &BTreeMap<String, String>, paths: &[String]) -> Library {
        let mut lib = Library::default();
        for path in paths {
            read_file(&mut lib, path, &files[path]).unwrap();
        }
        lib
    }

    #[test]
    fn blob_ids_match_git() {
        assert_eq!(blob_sha("hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
    }

    #[test]
    fn files_round_trip() {
        let lib = merge(&library(40), &library(40));
        let files = to_files(&lib);
        assert!(files.len() > 3 && files.len() <= SHARDS as usize + 2);
        assert!(files.keys().all(|p| is_data_file(p)));
        assert_eq!(files.values().filter(|c| c.contains("\"e7\"")).count(), 1);
        let paths: Vec<String> = files.keys().cloned().collect();
        assert_eq!(merge(&read_all(&files, &paths), &Library::default()), lib);
        assert_eq!(to_files(&lib), files);
    }

    #[test]
    fn only_changed_files_are_pulled_and_pushed() {
        let local = merge(&library(40), &library(40));
        let mut other = local.clone();
        other.entries[3].title = "edited".into();
        other.entries[3].modified = 99.0;
        let remote = to_files(&other);

        assert!(to_download(&to_files(&local), &shas(&to_files(&local))).is_empty());
        let paths = to_download(&to_files(&local), &shas(&remote));
        assert_eq!(paths, [shard(&other.entries[3].id)]);
        let merged = merge(&local, &read_all(&remote, &paths));
        assert_eq!(merged, merge(&local, &other));
        assert!(changes(&to_files(&merged), &shas(&remote)).is_empty());

        let mut fewer = merged.clone();
        fewer.delete_entries(&["e5".into()], 100.0);
        let changed: Vec<String> = changes(&to_files(&fewer), &shas(&remote))
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(changed, [DELETED.to_string(), shard("e5")]);
    }

    #[test]
    fn emptied_files_are_removed() {
        let mut lib = Library::default();
        lib.entries.push(entry("x", "", "K", 1.0, 1.0));
        let remote = shas(&to_files(&lib));
        lib.delete_entries(&["x".into()], 2.0);
        let removed: Vec<_> = changes(&to_files(&lib), &remote)
            .into_iter()
            .filter(|(_, c)| c.is_none())
            .map(|(p, _)| p)
            .collect();
        assert_eq!(removed, [shard("x")]);
    }
}
