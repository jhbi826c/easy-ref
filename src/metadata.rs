//! Converts Crossref and DataCite records into library entries.

use crate::doi;
use crate::model::{Entry, EntryType, Person};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub enum LookupError {
    NotFound,
    /// Network error or repeated rate limiting.
    Failed(String),
    Http(u16),
    Parse(String),
}

pub async fn lookup(doi: &str) -> Result<Entry, LookupError> {
    if doi::arxiv_id_from_doi(doi).is_none() {
        let url = format!("https://api.crossref.org/works/{}", encode_doi(doi));
        if let Some(json) = fetch_json(&url).await? {
            return Ok(from_crossref(&json["message"], doi));
        }
    }
    let url = format!("https://api.datacite.org/dois/{}", encode_doi(doi));
    match fetch_json(&url).await? {
        Some(json) => Ok(from_datacite(&json["data"]["attributes"], doi)),
        None => Err(LookupError::NotFound),
    }
}

/// `Ok(None)` on 404. Retries rate-limited requests, which browsers report as network errors.
async fn fetch_json(url: &str) -> Result<Option<Value>, LookupError> {
    const ATTEMPTS: u32 = 4;
    let mut last_error = String::new();
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            gloo_timers::future::TimeoutFuture::new(1000 << (attempt - 1)).await;
        }
        match gloo_net::http::Request::get(url).send().await {
            Ok(resp) => match resp.status() {
                200 => {
                    return resp
                        .json::<Value>()
                        .await
                        .map(Some)
                        .map_err(|e| LookupError::Parse(e.to_string()));
                }
                404 => return Ok(None),
                429 | 500..=599 => last_error = format!("HTTP {}", resp.status()),
                code => return Err(LookupError::Http(code)),
            },
            Err(e) => last_error = e.to_string(),
        }
    }
    Err(LookupError::Failed(last_error))
}

fn encode_doi(doi: &str) -> String {
    let mut out = String::new();
    for b in doi.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn from_crossref(msg: &Value, requested_doi: &str) -> Entry {
    let first = |key: &str| {
        msg[key]
            .get(0)
            .and_then(Value::as_str)
            .map(clean_text)
            .unwrap_or_default()
    };
    let text = |v: &Value| v.as_str().map(clean_text).unwrap_or_default();

    let mut title = first("title");
    let subtitle = first("subtitle");
    if !subtitle.is_empty() && !title.to_lowercase().contains(&subtitle.to_lowercase()) {
        title = format!("{title}: {subtitle}");
    }

    let authors = msg["author"]
        .as_array()
        .map(|a| a.iter().filter_map(crossref_person).collect())
        .unwrap_or_default();

    let (year, month) = ["issued", "published-print", "published-online", "published", "created"]
        .iter()
        .find_map(|k| date_parts(&msg[*k]))
        .unwrap_or((None, None));

    let containers: Vec<String> = msg["container-title"]
        .as_array()
        .map(|a| a.iter().map(text).filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let (mut venue, series) = split_containers(&containers);
    let acronym = msg["event"]["acronym"].as_str().map(clean_text);
    let has_event = msg["event"].is_object();
    let institution = msg["institution"].get(0).map(|i| text(&i["name"])).unwrap_or_default();

    let kind = match msg["type"].as_str().unwrap_or("") {
        "journal-article" => EntryType::Article,
        "proceedings-article" => EntryType::InProceedings,
        "book-chapter" | "book-section" | "book-part" => {
            if has_event || looks_like_proceedings(&venue) || series.starts_with("Lecture Notes") {
                EntryType::InProceedings
            } else {
                EntryType::InCollection
            }
        }
        "book" | "monograph" | "edited-book" | "reference-book" => EntryType::Book,
        "dissertation" => EntryType::PhdThesis,
        "report" | "report-component" => EntryType::TechReport,
        _ => EntryType::Misc,
    };

    let mut publisher = text(&msg["publisher"]);
    if matches!(kind, EntryType::PhdThesis | EntryType::TechReport) && !institution.is_empty() {
        publisher = institution.clone();
    }
    if kind == EntryType::Misc && venue.is_empty() {
        venue = if institution.is_empty() {
            publisher.clone()
        } else {
            institution
        };
    }

    let mut doi = text(&msg["DOI"]);
    if doi.is_empty() || doi.eq_ignore_ascii_case(requested_doi) {
        doi = requested_doi.to_string();
    }
    let mut pages = text(&msg["page"]);
    if pages.is_empty() {
        pages = text(&msg["article-number"]);
    }
    let url = text(&msg["URL"]);
    let url = if url.contains("doi.org/") { String::new() } else { url };
    let address = match kind {
        EntryType::Article | EntryType::Misc => String::new(),
        _ => publisher_address(&publisher).to_string(),
    };

    Entry {
        address,
        venue_short: venue_abbrev(&venue, acronym.as_deref(), &doi),
        kind,
        title,
        authors,
        year,
        month,
        venue,
        series,
        volume: text(&msg["volume"]),
        number: text(&msg["issue"]),
        pages,
        publisher,
        doi,
        url,
        ..Default::default()
    }
}

pub fn from_datacite(attrs: &Value, requested_doi: &str) -> Entry {
    let text = |v: &Value| v.as_str().map(clean_text).unwrap_or_default();

    let title = attrs["titles"].get(0).map(|t| text(&t["title"])).unwrap_or_default();
    let authors = attrs["creators"]
        .as_array()
        .map(|a| a.iter().filter_map(datacite_person).collect())
        .unwrap_or_default();
    let year = attrs["publicationYear"]
        .as_i64()
        .or_else(|| attrs["publicationYear"].as_str().and_then(|s| s.parse().ok()))
        .map(|y| y as i32);
    let month = attrs["dates"].as_array().and_then(|dates| {
        ["Available", "Submitted", "Issued"].iter().find_map(|kind| {
            dates.iter().filter(|d| d["dateType"] == *kind).find_map(|d| {
                let date = d["date"].as_str()?;
                let y: i32 = date.get(0..4)?.parse().ok()?;
                let m: u8 = date.get(5..7)?.parse().ok()?;
                (Some(y) == year && (1..=12).contains(&m)).then_some(m)
            })
        })
    });
    let publisher = match &attrs["publisher"] {
        Value::Object(o) => o.get("name").map(text).unwrap_or_default(),
        v => text(v),
    };

    let mut entry = Entry {
        title,
        authors,
        year,
        month,
        doi: requested_doi.to_string(),
        ..Default::default()
    };

    let arxiv_id = doi::arxiv_id_from_doi(requested_doi).map(str::to_string).or_else(|| {
        attrs["identifiers"]
            .as_array()?
            .iter()
            .find(|i| i["identifierType"] == "arXiv")
            .map(|i| text(&i["identifier"]))
    });
    if let Some(id) = arxiv_id.filter(|_| publisher.eq_ignore_ascii_case("arxiv")) {
        entry.kind = EntryType::Misc;
        entry.venue = "arXiv".into();
        entry.venue_short = "arXiv".into();
        entry.primary_class = attrs["subjects"]
            .as_array()
            .and_then(|s| s.iter().find_map(|s| arxiv_class(s["subject"].as_str()?)))
            .unwrap_or_default();
        entry.url = format!("https://arxiv.org/abs/{id}");
        entry.eprint = id;
        entry.publisher = publisher;
        return entry;
    }

    entry.kind = attrs["types"]["bibtex"]
        .as_str()
        .and_then(EntryType::from_bibtex_name)
        .unwrap_or(EntryType::Misc);
    entry.venue = attrs["container"]["title"]
        .as_str()
        .map(clean_text)
        .unwrap_or_else(|| publisher.clone());
    entry.venue_short = venue_abbrev(&entry.venue, None, requested_doi);
    entry.publisher = publisher;
    entry
}

fn crossref_person(v: &Value) -> Option<Person> {
    let get = |k: &str| v[k].as_str().map(clean_text).unwrap_or_default();
    let (family, given, name) = (get("family"), get("given"), get("name"));
    if !family.is_empty() {
        Some(Person {
            family,
            given,
            suffix: get("suffix"),
            literal: false,
        })
    } else if !name.is_empty() {
        Some(Person {
            family: name,
            literal: true,
            ..Default::default()
        })
    } else {
        None
    }
}

fn datacite_person(v: &Value) -> Option<Person> {
    let get = |k: &str| v[k].as_str().map(clean_text).unwrap_or_default();
    let (family, given, name) = (get("familyName"), get("givenName"), get("name"));
    if v["nameType"] == "Organizational" {
        return (!name.is_empty()).then(|| Person {
            family: name,
            literal: true,
            ..Default::default()
        });
    }
    if !family.is_empty() {
        return Some(Person {
            family,
            given,
            ..Default::default()
        });
    }
    Person::from_line(&name)
}

fn date_parts(v: &Value) -> Option<(Option<i32>, Option<u8>)> {
    let parts = v["date-parts"].get(0)?;
    let year = parts.get(0)?.as_i64()? as i32;
    let month = parts
        .get(1)
        .and_then(Value::as_i64)
        .filter(|m| (1..=12).contains(m))
        .map(|m| m as u8);
    Some((Some(year), month))
}

const SERIES_PREFIXES: [&str; 6] = [
    "Lecture Notes",
    "Communications in Computer and Information Science",
    "Advances in Intelligent Systems and Computing",
    "IFIP Advances",
    "Smart Innovation",
    "Studies in",
];

/// Returns `(book title, series)`.
fn split_containers(titles: &[String]) -> (String, String) {
    match titles {
        [] => (String::new(), String::new()),
        [only] => (only.clone(), String::new()),
        _ => {
            let is_series = |t: &String| SERIES_PREFIXES.iter().any(|p| t.starts_with(p));
            let series = titles.iter().find(|t| is_series(t)).cloned().unwrap_or_default();
            let book = titles
                .iter()
                .rev()
                .find(|t| !is_series(t))
                .cloned()
                .unwrap_or_else(|| titles[0].clone());
            (book, series)
        }
    }
}

const PUBLISHER_ADDRESSES: [(&str, &str); 13] = [
    ("association for computing machinery", "New York, NY, USA"),
    ("acm", "New York, NY, USA"),
    ("ieee computer society", "Los Alamitos, CA, USA"),
    ("ieee", "Piscataway, NJ, USA"),
    (
        "institute of electrical and electronics engineers",
        "Piscataway, NJ, USA",
    ),
    ("springer international publishing", "Cham"),
    ("springer berlin heidelberg", "Berlin, Heidelberg"),
    ("springer nature singapore", "Singapore"),
    ("association for computational linguistics", "Stroudsburg, PA, USA"),
    ("usenix", "Berkeley, CA, USA"),
    ("curran associates", "Red Hook, NY, USA"),
    ("mit press", "Cambridge, MA, USA"),
    ("cambridge university press", "Cambridge, UK"),
];

pub fn publisher_address(publisher: &str) -> &'static str {
    let p = publisher.to_lowercase();
    PUBLISHER_ADDRESSES
        .iter()
        .find(|(name, _)| p == *name || p.starts_with(&format!("{name} ")))
        .map(|(_, addr)| *addr)
        .unwrap_or("")
}

fn looks_like_proceedings(title: &str) -> bool {
    ["Proceedings", "Conference", "Workshop", "Symposium"]
        .iter()
        .any(|w| title.contains(w))
}

/// `Computation and Language (cs.CL)` -> `cs.CL`.
fn arxiv_class(subject: &str) -> Option<String> {
    let inner = subject.rsplit_once('(')?.1.strip_suffix(')')?;
    (inner.contains('.') || inner.contains('-')).then(|| inner.to_string())
}

/// Removes markup, decodes HTML entities and collapses whitespace.
pub fn clean_text(s: &str) -> String {
    let mut stripped = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => stripped.push(c),
            _ => {}
        }
    }
    let decoded = decode_entities(&stripped);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &rest[1..end];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ if name.starts_with("#x") || name.starts_with("#X") => {
                    u32::from_str_radix(&name[2..], 16).ok().and_then(char::from_u32)
                }
                _ if name.starts_with('#') => name[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

const PUBLISHERS: [&str; 9] = ["IEEE", "ACM", "CVF", "SIAM", "IAPR", "IFIP", "IET", "AAAS", "SPIE"];
const STOPWORDS: [&str; 14] = [
    "of", "the", "on", "and", "for", "in", "a", "an", "to", "at", "with", "&", "de", "und",
];

const KNOWN_VENUES: [(&str, &str); 8] = [
    ("acm transactions on graphics", "TOG"),
    ("acm computing surveys", "CSUR"),
    ("proceedings of the ieee", "ProcIEEE"),
    ("advances in neural information processing systems", "NeurIPS"),
    ("nature communications", "NatCommun"),
    ("nature machine intelligence", "NatMachIntell"),
    ("science advances", "SciAdv"),
    ("proceedings of the national academy of sciences", "PNAS"),
];

/// Distinctive parts of conference names that are often written out in full.
const CONFERENCES: [(&str, &str); 40] = [
    ("computer vision and pattern recognition", "CVPR"),
    ("international conference on computer vision", "ICCV"),
    ("european conference on computer vision", "ECCV"),
    ("asian conference on computer vision", "ACCV"),
    ("winter conference on applications of computer vision", "WACV"),
    ("british machine vision conference", "BMVC"),
    ("neural information processing systems", "NeurIPS"),
    ("international conference on machine learning", "ICML"),
    ("international conference on learning representations", "ICLR"),
    ("international joint conference on artificial intelligence", "IJCAI"),
    ("aaai conference on artificial intelligence", "AAAI"),
    (
        "north american chapter of the association for computational linguistics",
        "NAACL",
    ),
    (
        "european chapter of the association for computational linguistics",
        "EACL",
    ),
    ("annual meeting of the association for computational linguistics", "ACL"),
    ("empirical methods in natural language processing", "EMNLP"),
    ("international conference on computational linguistics", "COLING"),
    ("knowledge discovery and data mining", "KDD"),
    ("knowledge discovery & data mining", "KDD"),
    ("the web conference", "WWW"),
    ("world wide web conference", "WWW"),
    ("research and development in information retrieval", "SIGIR"),
    ("human factors in computing systems", "CHI"),
    ("user interface software and technology", "UIST"),
    ("computer-supported cooperative work", "CSCW"),
    ("international conference on multimedia", "MM"),
    ("international conference on robotics and automation", "ICRA"),
    ("intelligent robots and systems", "IROS"),
    ("computer and communications security", "CCS"),
    ("symposium on security and privacy", "SP"),
    ("operating systems design and implementation", "OSDI"),
    ("symposium on operating systems principles", "SOSP"),
    ("networked systems design and implementation", "NSDI"),
    ("international conference on software engineering", "ICSE"),
    ("foundations of software engineering", "FSE"),
    ("programming language design and implementation", "PLDI"),
    ("principles of programming languages", "POPL"),
    ("international symposium on computer architecture", "ISCA"),
    (
        "architectural support for programming languages and operating systems",
        "ASPLOS",
    ),
    ("international conference on management of data", "SIGMOD"),
    ("medical image computing and computer assisted intervention", "MICCAI"),
];

/// Short venue name used in citation keys, e.g. `CVPR`, `TPAMI`, `CHI`.
pub fn venue_abbrev(venue: &str, event_acronym: Option<&str>, doi: &str) -> String {
    if doi::arxiv_id_from_doi(doi).is_some() {
        return "arXiv".into();
    }
    event_acronym
        .and_then(clean_acronym)
        .or_else(|| acl_venue(doi))
        .or_else(|| parenthesized_acronym(venue))
        .or_else(|| {
            let lower = venue.to_lowercase();
            CONFERENCES
                .iter()
                .find(|(phrase, _)| lower.contains(phrase))
                .or_else(|| KNOWN_VENUES.iter().find(|(name, _)| lower.starts_with(name)))
                .map(|(_, abbr)| abbr.to_string())
        })
        .or_else(|| caps_token(venue))
        .unwrap_or_else(|| initials(venue))
}

/// `CHI '19` -> `CHI`, `SIGMOD/PODS '20` -> `SIGMOD`.
fn clean_acronym(acronym: &str) -> Option<String> {
    let head = acronym.split(['\'', '’', '/']).next().unwrap_or("");
    let s: String = head
        .split_whitespace()
        .filter(|w| !w.bytes().all(|b| b.is_ascii_digit()))
        .flat_map(|w| w.chars().filter(|c| c.is_alphanumeric()))
        .collect();
    (s.chars().filter(|c| c.is_alphabetic()).count() >= 2).then_some(s)
}

/// ACL Anthology DOIs: `10.18653/v1/N19-1423`, `10.18653/v1/2020.acl-main.1`.
fn acl_venue(doi: &str) -> Option<String> {
    let doi = doi.to_ascii_lowercase();
    let id = doi.strip_prefix("10.18653/v1/")?;
    if let Some((_, rest)) = id.split_once('.').filter(|(y, _)| y.len() == 4) {
        let event = rest.split('.').next()?;
        let mut parts = event.split('-');
        let first = parts.next()?;
        return Some(if first == "findings" {
            format!("Findings{}", parts.next()?.to_uppercase())
        } else {
            first.to_uppercase()
        });
    }
    let venue = match id.chars().next()?.to_ascii_uppercase() {
        'P' => "ACL",
        'N' => "NAACL",
        'D' => "EMNLP",
        'E' => "EACL",
        'C' => "COLING",
        'Q' => "TACL",
        'J' => "CL",
        'K' => "CoNLL",
        'S' => "SemEval",
        'W' => "WS",
        _ => return None,
    };
    Some(venue.to_string())
}

const ROMAN: [&str; 10] = ["II", "III", "IV", "VI", "VII", "VIII", "IX", "XI", "XII", "XIII"];

fn is_acronym(word: &str) -> bool {
    let upper = word.chars().filter(|c| c.is_uppercase()).count();
    upper >= 2 && !ROMAN.contains(&word) && word.chars().all(|c| c.is_uppercase() || c.is_ascii_digit())
}

/// `... Recognition (CVPR)` -> `CVPR`.
fn parenthesized_acronym(venue: &str) -> Option<String> {
    let mut rest = venue;
    let mut found = None;
    while let Some(open) = rest.find('(') {
        let Some(close) = rest[open..].find(')') else { break };
        let inner = &rest[open + 1..open + close];
        let acr: String = inner.split_whitespace().filter(|w| is_acronym(w)).collect();
        if !acr.is_empty() {
            found = Some(acr);
        }
        rest = &rest[open + close + 1..];
    }
    found
}

/// First standalone acronym that is not a publisher, e.g. `Computer Vision – ECCV 2016` -> `ECCV`.
fn caps_token(venue: &str) -> Option<String> {
    venue
        .split(|c: char| c.is_whitespace() || c == '/' || c == ',' || c == ':')
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .find(|w| is_acronym(w) && !PUBLISHERS.contains(w) && !w.chars().any(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

/// `IEEE Transactions on Pattern Analysis and Machine Intelligence` -> `TPAMI`.
fn initials(venue: &str) -> String {
    let mut words: Vec<&str> = venue
        .split(|c: char| c.is_whitespace() || c == '-' || c == '–' || c == ':' || c == ',')
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty())
        .filter(|w| !STOPWORDS.contains(&w.to_lowercase().as_str()))
        .filter(|w| !w.chars().any(|c| c.is_ascii_digit()))
        .filter(|w| *w != "Proceedings")
        .collect();
    let capitalize = |w: &str| {
        let mut c = w.chars();
        c.next()
            .map(|f| f.to_uppercase().chain(c).collect::<String>())
            .unwrap_or_default()
    };
    let mut prefix = String::new();
    if words.len() >= 2 && PUBLISHERS.contains(&words[0]) {
        prefix = words.remove(0).to_string();
        if words.len() == 1 {
            return format!("{prefix}{}", capitalize(words[0]));
        }
    }
    match words.as_slice() {
        [] => prefix,
        [only] => capitalize(only),
        _ => words
            .iter()
            .map(|w| {
                if is_acronym(w) {
                    w.to_string()
                } else {
                    w.chars().next().unwrap().to_uppercase().collect()
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crossref(name: &str) -> Entry {
        let raw = match name {
            "cvpr" => include_str!("../tests/fixtures/crossref-cvpr.json"),
            "tpami" => include_str!("../tests/fixtures/crossref-tpami.json"),
            "chi" => include_str!("../tests/fixtures/crossref-chi.json"),
            "cacm" => include_str!("../tests/fixtures/crossref-cacm.json"),
            "eccv" => include_str!("../tests/fixtures/crossref-eccv.json"),
            "naacl" => include_str!("../tests/fixtures/crossref-naacl.json"),
            "acl" => include_str!("../tests/fixtures/crossref-acl.json"),
            "tog" => include_str!("../tests/fixtures/crossref-tog.json"),
            "nature" => include_str!("../tests/fixtures/crossref-nature.json"),
            "access" => include_str!("../tests/fixtures/crossref-access.json"),
            _ => unreachable!(),
        };
        let json: Value = serde_json::from_str(raw).unwrap();
        from_crossref(&json["message"], "")
    }

    #[test]
    fn crossref_venues_and_types() {
        let cases = [
            ("cvpr", EntryType::InProceedings, "CVPR", 2016),
            ("tpami", EntryType::Article, "TPAMI", 2017),
            ("chi", EntryType::InProceedings, "CHI", 2019),
            ("cacm", EntryType::Article, "CACM", 2017),
            ("eccv", EntryType::InProceedings, "ECCV", 2016),
            ("naacl", EntryType::InProceedings, "NAACL", 2019),
            ("acl", EntryType::InProceedings, "ACL", 2020),
            ("tog", EntryType::Article, "TOG", 2021),
            ("nature", EntryType::Article, "Nature", 2015),
            ("access", EntryType::Article, "IEEEAccess", 2019),
        ];
        for (name, kind, short, year) in cases {
            let e = crossref(name);
            assert_eq!(
                (e.kind, e.venue_short.as_str(), e.year),
                (kind, short, Some(year)),
                "{name}"
            );
            assert!(!e.title.is_empty() && !e.authors.is_empty(), "{name}");
        }
    }

    #[test]
    fn crossref_fields() {
        let e = crossref("cvpr");
        assert_eq!(e.title, "Deep Residual Learning for Image Recognition");
        assert_eq!(
            e.authors[0],
            Person {
                family: "He".into(),
                given: "Kaiming".into(),
                ..Default::default()
            }
        );
        assert_eq!((e.pages.as_str(), e.month), ("770-778", Some(6)));
        assert_eq!(e.doi, "10.1109/cvpr.2016.90");
        let json: Value = serde_json::from_str(include_str!("../tests/fixtures/crossref-cvpr.json")).unwrap();
        assert_eq!(
            from_crossref(&json["message"], "10.1109/CVPR.2016.90").doi,
            "10.1109/CVPR.2016.90"
        );

        let e = crossref("eccv");
        assert_eq!(e.venue, "Computer Vision – ECCV 2016");
        assert_eq!(e.series, "Lecture Notes in Computer Science");
    }

    #[test]
    fn datacite_arxiv() {
        let json: Value = serde_json::from_str(include_str!("../tests/fixtures/datacite-arxiv.json")).unwrap();
        let e = from_datacite(&json["data"]["attributes"], "10.48550/arXiv.1706.03762");
        assert_eq!(e.title, "Attention Is All You Need");
        assert_eq!(e.kind, EntryType::Misc);
        assert_eq!((e.eprint.as_str(), e.primary_class.as_str()), ("1706.03762", "cs.CL"));
        assert_eq!((e.year, e.month), (Some(2017), Some(6)));
        assert_eq!(e.authors.len(), 8);
        assert_eq!(e.authors[0].family, "Vaswani");
        assert_eq!(e.venue_short, "arXiv");
    }

    #[test]
    fn cleans_markup() {
        assert_eq!(
            clean_text("A <i>novel</i>\n  method &amp; more&#x2014;fast"),
            "A novel method & more—fast"
        );
        assert_eq!(clean_text("R&D 5 &lt; 6"), "R&D 5 < 6");
    }

    #[test]
    fn spelled_out_conference_names() {
        let cases = [
            (
                "Proceedings of the IEEE conference on computer vision and pattern recognition",
                "CVPR",
            ),
            (
                "Proceedings of the IEEE/CVF international conference on computer vision",
                "ICCV",
            ),
            ("International conference on machine learning", "ICML"),
            (
                "Proceedings of the 2019 Conference of the North American Chapter of the Association for Computational Linguistics",
                "NAACL",
            ),
            (
                "Proceedings of the 58th Annual Meeting of the Association for Computational Linguistics",
                "ACL",
            ),
            (
                "2016 {IEEE} Conference on Computer Vision and Pattern Recognition, {CVPR} 2016, Las Vegas, NV, USA",
                "CVPR",
            ),
            (
                "Proceedings of the 25th ACM SIGKDD International Conference on Knowledge Discovery & Data Mining",
                "KDD",
            ),
        ];
        for (venue, want) in cases {
            assert_eq!(venue_abbrev(venue, None, ""), want, "{venue}");
        }
    }

    #[test]
    fn abbreviations() {
        assert_eq!(
            venue_abbrev("International Journal of Computer Vision", None, ""),
            "IJCV"
        );
        assert_eq!(venue_abbrev("Journal of Machine Learning Research", None, ""), "JMLR");
        assert_eq!(
            venue_abbrev(
                "Proceedings of the AAAI Conference on Artificial Intelligence",
                None,
                ""
            ),
            "AAAI"
        );
        assert_eq!(
            venue_abbrev(
                "2019 IEEE/CVF International Conference on Computer Vision (ICCV)",
                None,
                ""
            ),
            "ICCV"
        );
        assert_eq!(venue_abbrev("x", Some("MM '19"), ""), "MM");
        assert_eq!(
            venue_abbrev("x", None, "10.18653/v1/2023.findings-emnlp.1"),
            "FindingsEMNLP"
        );
    }
}
