use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    Article,
    InProceedings,
    InCollection,
    Book,
    PhdThesis,
    TechReport,
    #[default]
    Misc,
}

impl EntryType {
    pub const ALL: [EntryType; 7] = [
        EntryType::Article,
        EntryType::InProceedings,
        EntryType::InCollection,
        EntryType::Book,
        EntryType::PhdThesis,
        EntryType::TechReport,
        EntryType::Misc,
    ];

    pub fn bibtex_name(self) -> &'static str {
        match self {
            EntryType::Article => "article",
            EntryType::InProceedings => "inproceedings",
            EntryType::InCollection => "incollection",
            EntryType::Book => "book",
            EntryType::PhdThesis => "phdthesis",
            EntryType::TechReport => "techreport",
            EntryType::Misc => "misc",
        }
    }

    pub fn from_bibtex_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.bibtex_name() == name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Person {
    pub family: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub given: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub suffix: String,
    /// Organization name, never split into given/family.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub literal: bool,
}

impl Person {
    /// One line of the author editor: `Family, Given`, `Family, Jr., Given` or `{Organization}`.
    pub fn to_line(&self) -> String {
        if self.literal {
            format!("{{{}}}", self.family)
        } else if self.given.is_empty() {
            self.family.clone()
        } else if self.suffix.is_empty() {
            format!("{}, {}", self.family, self.given)
        } else {
            format!("{}, {}, {}", self.family, self.suffix, self.given)
        }
    }

    pub fn from_line(line: &str) -> Option<Person> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        if let Some(inner) = line.strip_prefix('{').and_then(|l| l.strip_suffix('}')) {
            return Some(Person {
                family: inner.trim().to_string(),
                literal: true,
                ..Default::default()
            });
        }
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        let person = match parts.as_slice() {
            [family] => match family.rsplit_once(' ') {
                Some((given, family)) => Person {
                    family: family.into(),
                    given: given.into(),
                    ..Default::default()
                },
                None => Person {
                    family: family.to_string(),
                    ..Default::default()
                },
            },
            [family, given] => Person {
                family: family.to_string(),
                given: given.to_string(),
                ..Default::default()
            },
            [family, suffix, given, ..] => Person {
                family: family.to_string(),
                given: given.to_string(),
                suffix: suffix.to_string(),
                literal: false,
            },
            [] => return None,
        };
        Some(person)
    }

    pub fn display(&self) -> String {
        if self.given.is_empty() {
            self.family.clone()
        } else {
            format!("{} {}", self.given, self.family)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub id: String,
    pub key: String,
    pub kind: EntryType,
    pub title: String,
    pub authors: Vec<Person>,
    pub year: Option<i32>,
    pub month: Option<u8>,
    /// Journal name or proceedings title.
    pub venue: String,
    /// Short venue name used in the citation key, e.g. `CVPR`.
    pub venue_short: String,
    pub series: String,
    pub volume: String,
    pub number: String,
    pub pages: String,
    pub publisher: String,
    /// Publisher location.
    pub address: String,
    pub doi: String,
    pub url: String,
    /// arXiv identifier, e.g. `1706.03762`.
    pub eprint: String,
    pub primary_class: String,
    pub added: f64,
    pub modified: f64,
}

impl Entry {
    pub fn first_author_family(&self) -> &str {
        self.authors.first().map(|p| p.family.as_str()).unwrap_or("")
    }

    pub fn authors_display(&self) -> String {
        match self.authors.len() {
            0 => String::new(),
            1 => self.authors[0].display(),
            2 => format!("{} and {}", self.authors[0].display(), self.authors[1].display()),
            _ => format!("{} et al.", self.authors[0].display()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub entries: Vec<String>,
    /// Position in the sidebar.
    pub order: f64,
    pub modified: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub collections: Vec<Collection>,
    /// Deleted entry/collection ids with deletion time, kept for sync.
    pub deleted: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Library,
    Collection(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Added(String),
    Duplicate(String),
}

pub fn normalize_doi(doi: &str) -> String {
    doi.trim().to_ascii_lowercase()
}

fn normalize_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

impl Library {
    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn collection(&self, id: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.id == id)
    }

    fn collection_mut(&mut self, id: &str) -> Option<&mut Collection> {
        self.collections.iter_mut().find(|c| c.id == id)
    }

    /// Same DOI, or same title and first author.
    pub fn find_duplicate(&self, candidate: &Entry) -> Option<&Entry> {
        let doi = normalize_doi(&candidate.doi);
        let title = normalize_title(&candidate.title);
        let family = candidate.first_author_family().to_lowercase();
        self.entries.iter().find(|e| {
            if e.id == candidate.id {
                return false;
            }
            if !doi.is_empty() && normalize_doi(&e.doi) == doi {
                return true;
            }
            !title.is_empty() && normalize_title(&e.title) == title && e.first_author_family().to_lowercase() == family
        })
    }

    pub fn find_by_doi(&self, doi: &str) -> Option<&Entry> {
        let doi = normalize_doi(doi);
        self.entries.iter().find(|e| normalize_doi(&e.doi) == doi)
    }

    /// Appends `a`, `b`, ... to `base` until no other entry uses it.
    pub fn unique_key(&self, base: &str, except_id: &str) -> String {
        let taken: HashSet<&str> = self
            .entries
            .iter()
            .filter(|e| e.id != except_id)
            .map(|e| e.key.as_str())
            .collect();
        if !taken.contains(base) {
            return base.to_string();
        }
        let mut n = 0usize;
        loop {
            let candidate = format!("{base}{}", alpha_suffix(n));
            if !taken.contains(candidate.as_str()) {
                return candidate;
            }
            n += 1;
        }
    }

    pub fn add_entry(&mut self, mut entry: Entry, now: f64) -> AddOutcome {
        if let Some(existing) = self.find_duplicate(&entry) {
            return AddOutcome::Duplicate(existing.id.clone());
        }
        entry.key = self.unique_key(&entry.key, &entry.id);
        entry.added = now;
        entry.modified = now;
        let id = entry.id.clone();
        self.entries.push(entry);
        AddOutcome::Added(id)
    }

    pub fn update_entry(&mut self, mut entry: Entry, now: f64) {
        entry.key = self.unique_key(&entry.key, &entry.id);
        entry.modified = now;
        if let Some(slot) = self.entries.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry;
        }
    }

    pub fn delete_entries(&mut self, ids: &[String], now: f64) {
        let ids: HashSet<&str> = ids.iter().map(String::as_str).collect();
        self.entries.retain(|e| !ids.contains(e.id.as_str()));
        for c in &mut self.collections {
            let before = c.entries.len();
            c.entries.retain(|id| !ids.contains(id.as_str()));
            if c.entries.len() != before {
                c.modified = now;
            }
        }
        for id in ids {
            self.deleted.insert(id.to_string(), now);
        }
    }

    pub fn create_collection(&mut self, id: String, name: &str, now: f64) {
        let order = self.collections.iter().map(|c| c.order + 1.0).fold(0.0, f64::max);
        self.collections.push(Collection {
            id,
            name: name.trim().to_string(),
            entries: vec![],
            order,
            modified: now,
        });
    }

    /// Moves collection `id` before or after `target`.
    pub fn move_collection(&mut self, id: &str, target: &str, after: bool, now: f64) {
        if id == target {
            return;
        }
        let Some(from) = self.collections.iter().position(|c| c.id == id) else {
            return;
        };
        let moved = self.collections.remove(from);
        let Some(to) = self.collections.iter().position(|c| c.id == target) else {
            self.collections.insert(from, moved);
            return;
        };
        self.collections.insert(if after { to + 1 } else { to }, moved);
        for (i, c) in self.collections.iter_mut().enumerate() {
            if c.order != i as f64 {
                c.order = i as f64;
                c.modified = now;
            }
        }
    }

    pub fn rename_collection(&mut self, id: &str, name: &str, now: f64) {
        if let Some(c) = self.collection_mut(id) {
            c.name = name.trim().to_string();
            c.modified = now;
        }
    }

    pub fn delete_collection(&mut self, id: &str, now: f64) {
        self.collections.retain(|c| c.id != id);
        self.deleted.insert(id.to_string(), now);
    }

    /// Returns how many entries were newly added.
    pub fn add_to_collection(&mut self, collection_id: &str, ids: &[String], now: f64) -> usize {
        let Some(c) = self.collection_mut(collection_id) else {
            return 0;
        };
        let mut added = 0;
        for id in ids {
            if !c.entries.contains(id) {
                c.entries.push(id.clone());
                added += 1;
            }
        }
        if added > 0 {
            c.modified = now;
        }
        added
    }

    pub fn remove_from_collection(&mut self, collection_id: &str, ids: &[String], now: f64) {
        if let Some(c) = self.collection_mut(collection_id) {
            c.entries.retain(|id| !ids.contains(id));
            c.modified = now;
        }
    }

    pub fn entries_in(&self, view: &View) -> Vec<&Entry> {
        match view {
            View::Library => self.entries.iter().collect(),
            View::Collection(cid) => match self.collection(cid) {
                Some(c) => c.entries.iter().filter_map(|id| self.entry(id)).collect(),
                None => vec![],
            },
        }
    }
}

pub fn alpha_suffix(mut n: usize) -> String {
    let mut s = Vec::new();
    loop {
        s.push(b'a' + (n % 26) as u8);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, doi: &str, title: &str, family: &str, key: &str) -> Entry {
        Entry {
            id: id.into(),
            doi: doi.into(),
            title: title.into(),
            key: key.into(),
            authors: vec![Person {
                family: family.into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn dedupes_by_doi_and_title() {
        let mut lib = Library::default();
        assert!(matches!(
            lib.add_entry(entry("1", "10.1/ABC", "Deep Learning", "LeCun", "k"), 0.0),
            AddOutcome::Added(_)
        ));
        assert_eq!(
            lib.add_entry(entry("2", "10.1/abc", "Other", "X", "k"), 0.0),
            AddOutcome::Duplicate("1".into())
        );
        assert_eq!(
            lib.add_entry(entry("3", "", "Deep  learning!", "lecun", "k"), 0.0),
            AddOutcome::Duplicate("1".into())
        );
        assert!(matches!(
            lib.add_entry(entry("4", "", "Deep Learning", "Bengio", "k"), 0.0),
            AddOutcome::Added(_)
        ));
        assert_eq!(lib.entries.len(), 2);
    }

    #[test]
    fn unique_keys_get_suffixes() {
        let mut lib = Library::default();
        for (i, t) in ["A", "B", "C"].iter().enumerate() {
            lib.add_entry(entry(&i.to_string(), "", t, "He", "He2016CVPR"), 0.0);
        }
        let keys: Vec<_> = lib.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, ["He2016CVPR", "He2016CVPRa", "He2016CVPRb"]);
        assert_eq!(alpha_suffix(26), "aa");
    }

    #[test]
    fn person_lines_round_trip() {
        for line in [
            "He, Kaiming",
            "van der Maaten, Laurens",
            "King, Jr., Martin Luther",
            "{OpenAI}",
            "Plato",
        ] {
            assert_eq!(Person::from_line(line).unwrap().to_line(), line);
        }
        assert_eq!(Person::from_line("Kaiming He").unwrap().to_line(), "He, Kaiming");
    }

    #[test]
    fn deleting_entry_removes_membership() {
        let mut lib = Library::default();
        lib.add_entry(entry("e", "", "T", "A", "k"), 0.0);
        lib.create_collection("c".into(), "C", 0.0);
        lib.add_to_collection("c", &["e".into()], 1.0);
        lib.delete_entries(&["e".into()], 2.0);
        assert!(lib.collection("c").unwrap().entries.is_empty());
        assert_eq!(lib.deleted.get("e"), Some(&2.0));
    }

    #[test]
    fn moves_collections() {
        let mut lib = Library::default();
        for id in ["a", "b", "c", "d"] {
            lib.create_collection(id.into(), id, 0.0);
        }
        let ids = |lib: &Library| lib.collections.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        lib.move_collection("d", "a", false, 1.0);
        assert_eq!(ids(&lib), ["d", "a", "b", "c"]);
        lib.move_collection("d", "b", true, 2.0);
        assert_eq!(ids(&lib), ["a", "b", "d", "c"]);
        let orders: Vec<f64> = lib.collections.iter().map(|c| c.order).collect();
        assert_eq!(orders, [0.0, 1.0, 2.0, 3.0]);
        lib.create_collection("e".into(), "e", 3.0);
        assert_eq!(lib.collections.last().unwrap().order, 4.0);
    }
}
