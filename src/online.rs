//! Online entries created from pasted web links.

use crate::metadata::LookupError;
use crate::model::{Entry, EntryType, Person};
use serde_json::Value;

/// Hosts of paper pages whose links do not contain a DOI.
const PAPER_HOSTS: [&str; 16] = [
    "ieeexplore.ieee.org",
    "sciencedirect.com",
    "link.springer.com",
    "dl.acm.org",
    "onlinelibrary.wiley.com",
    "openreview.net",
    "proceedings.neurips.cc",
    "papers.nips.cc",
    "proceedings.mlr.press",
    "aclanthology.org",
    "scholar.google.com",
    "semanticscholar.org",
    "researchgate.net",
    "jstor.org",
    "nature.com",
    "science.org",
];

const SITE_NAMES: [(&str, &str); 16] = [
    ("github.com", "GitHub"),
    ("gitlab.com", "GitLab"),
    ("huggingface.co", "Hugging Face"),
    ("wikipedia.org", "Wikipedia"),
    ("youtube.com", "YouTube"),
    ("medium.com", "Medium"),
    ("stackoverflow.com", "Stack Overflow"),
    ("pypi.org", "PyPI"),
    ("crates.io", "crates.io"),
    ("docs.rs", "Docs.rs"),
    ("npmjs.com", "npm"),
    ("pytorch.org", "PyTorch"),
    ("tensorflow.org", "TensorFlow"),
    ("kaggle.com", "Kaggle"),
    ("x.com", "X"),
    ("twitter.com", "X"),
];

/// Lowercase host of an `http(s)` link without `www.` and port.
pub fn host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?.split(':').next()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    host.contains('.').then_some(host)
}

fn on_host(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

pub fn is_paper_page(host: &str) -> bool {
    PAPER_HOSTS.iter().any(|d| on_host(host, d))
}

/// Display name of a website, e.g. `GitHub` or `rust-lang.org`.
pub fn site_name(host: &str) -> String {
    SITE_NAMES
        .iter()
        .find(|(d, _)| on_host(host, d))
        .map_or_else(|| host.to_string(), |(_, name)| name.to_string())
}

/// Short site name for citation keys, e.g. `GitHub` or `RustLang`.
pub fn site_short(host: &str) -> String {
    if let Some((_, name)) = SITE_NAMES.iter().find(|(d, _)| on_host(host, d)) {
        return name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    }
    let parts: Vec<&str> = host.split('.').collect();
    let main = if parts.len() >= 2 { parts[parts.len() - 2] } else { host };
    main.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// Link with scheme, `www.` and trailing slash removed, for duplicate checks.
pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    rest.trim_end_matches('/').to_lowercase()
}

/// `(owner, repo)` of a GitHub repository link.
pub fn github_repo(url: &str) -> Option<(String, String)> {
    if host(url)? != "github.com" {
        return None;
    }
    let path = url.split("github.com/").nth(1)?;
    let mut parts = path.split(['/', '?', '#']).filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let repo = parts.next()?.trim_end_matches(".git");
    Some((owner.to_string(), repo.to_string()))
}

/// An online entry with only the link, site and access date filled in.
pub fn link_entry(url: &str, today: &str, year: i32) -> Entry {
    let host = host(url).unwrap_or_default();
    Entry {
        kind: EntryType::Online,
        url: url.trim().to_string(),
        urldate: today.to_string(),
        year: Some(year),
        venue: site_name(&host),
        venue_short: site_short(&host),
        ..Default::default()
    }
}

/// Fills an entry from GitHub's repository and owner records.
pub fn from_github(repo: &Value, owner: &Value, mut entry: Entry) -> Entry {
    let name = repo["name"].as_str().unwrap_or_default();
    entry.title = match repo["description"].as_str().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => format!("{name}: {}", d.trim_end_matches('.')),
        None => name.to_string(),
    };
    let owner_name = owner["name"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
        .or_else(|| repo["owner"]["login"].as_str())
        .unwrap_or_default();
    entry.authors = vec![Person {
        family: owner_name.to_string(),
        literal: true,
        ..Default::default()
    }];
    if let Some(year) = repo["pushed_at"].as_str().and_then(|d| d.get(0..4)?.parse().ok()) {
        entry.year = Some(year);
    }
    if let Some(url) = repo["html_url"].as_str() {
        entry.url = url.to_string();
    }
    entry
}

pub async fn lookup_github(owner: &str, repo: &str, token: &str, entry: Entry) -> Result<Entry, LookupError> {
    let repo_json = get_json(&format!("https://api.github.com/repos/{owner}/{repo}"), token).await?;
    let login = repo_json["owner"]["login"].as_str().unwrap_or(owner).to_string();
    let owner_json = get_json(&format!("https://api.github.com/users/{login}"), token)
        .await
        .unwrap_or(Value::Null);
    Ok(from_github(&repo_json, &owner_json, entry))
}

async fn get_json(url: &str, token: &str) -> Result<Value, LookupError> {
    let mut request = gloo_net::http::Request::get(url).header("Accept", "application/vnd.github+json");
    if !token.is_empty() {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let resp = request.send().await.map_err(|e| LookupError::Failed(e.to_string()))?;
    match resp.status() {
        200 => resp.json().await.map_err(|e| LookupError::Parse(e.to_string())),
        404 => Err(LookupError::NotFound),
        code => Err(LookupError::Http(code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_sites() {
        assert_eq!(
            host("https://www.Rust-Lang.org/policies?x=1").as_deref(),
            Some("rust-lang.org")
        );
        assert_eq!(host("http://localhost:8080/"), None);
        assert_eq!(host("ftp://x.org"), None);
        assert!(is_paper_page("ieeexplore.ieee.org"));
        assert!(is_paper_page("www.sciencedirect.com".trim_start_matches("www.")));
        assert!(!is_paper_page("github.com"));
        assert_eq!(site_name("en.wikipedia.org"), "Wikipedia");
        assert_eq!(site_name("rust-lang.org"), "rust-lang.org");
        assert_eq!(
            (site_short("rust-lang.org"), site_short("huggingface.co")),
            ("RustLang".into(), "HuggingFace".into())
        );
        assert_eq!(
            normalize_url("https://www.github.com/leptos-rs/leptos/"),
            normalize_url("http://github.com/Leptos-rs/leptos")
        );
    }

    #[test]
    fn github_links() {
        assert_eq!(
            github_repo("https://github.com/leptos-rs/leptos"),
            Some(("leptos-rs".into(), "leptos".into()))
        );
        assert_eq!(
            github_repo("https://github.com/leptos-rs/leptos/tree/main/examples"),
            Some(("leptos-rs".into(), "leptos".into()))
        );
        assert_eq!(github_repo("https://github.com/leptos-rs"), None);
        assert_eq!(github_repo("https://gitlab.com/a/b"), None);
    }

    #[test]
    fn github_metadata() {
        let repo: Value = serde_json::from_str(
            r#"{"name":"leptos","description":"Build fast web applications with Rust.","owner":{"login":"leptos-rs"},
                "pushed_at":"2026-10-20T10:00:00Z","html_url":"https://github.com/leptos-rs/leptos"}"#,
        )
        .unwrap();
        let owner: Value = serde_json::from_str(r#"{"login":"leptos-rs","name":"Leptos"}"#).unwrap();
        let e = from_github(
            &repo,
            &owner,
            link_entry("https://github.com/leptos-rs/leptos", "2026-10-28", 2026),
        );
        assert_eq!(e.title, "leptos: Build fast web applications with Rust");
        assert_eq!((e.authors[0].family.as_str(), e.authors[0].literal), ("Leptos", true));
        assert_eq!(
            (e.year, e.venue.as_str(), e.urldate.as_str()),
            (Some(2026), "GitHub", "2026-10-28")
        );
    }
}
