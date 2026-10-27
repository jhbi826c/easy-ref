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

    /// BibTeX's `and others`, rendered as "et al." by styles.
    pub fn is_others(&self) -> bool {
        !self.literal && self.given.is_empty() && self.family == "others"
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
    /// When the entry was moved to the trash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trashed: Option<f64>,
}

impl Entry {
    pub fn first_author_family(&self) -> &str {
        self.authors.first().map(|p| p.family.as_str()).unwrap_or("")
    }

    pub fn authors_display(&self) -> String {
        let truncated = self.authors.iter().any(Person::is_others);
        let named: Vec<&Person> = self.authors.iter().filter(|p| !p.is_others()).collect();
        match named.as_slice() {
            [] => String::new(),
            [only] if !truncated => only.display(),
            [a, b] if !truncated => format!("{} and {}", a.display(), b.display()),
            [first, ..] => format!("{} et al.", first.display()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Collection {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub entries: Vec<String>,
    /// Position among its siblings.
    pub order: f64,
    pub modified: f64,
}

/// Where a dragged collection lands relative to the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Before,
    After,
    Inside,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub collections: Vec<Collection>,
    /// Deleted entry and collection ids with deletion time.
    pub deleted: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    Library,
    Collection(String),
    Trash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddOutcome {
    Added(String),
    Duplicate(String),
    /// Same paper with a different DOI; the existing entry now holds the new metadata.
    Replaced {
        id: String,
        old_key: String,
    },
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

    /// An entry outside the trash with the same DOI, or the same title and first author.
    pub fn find_duplicate(&self, candidate: &Entry) -> Option<&Entry> {
        let doi = normalize_doi(&candidate.doi);
        let title = normalize_title(&candidate.title);
        let family = candidate.first_author_family().to_lowercase();
        self.entries.iter().find(|e| {
            if e.id == candidate.id || e.trashed.is_some() {
                return false;
            }
            if !doi.is_empty() && normalize_doi(&e.doi) == doi {
                return true;
            }
            !title.is_empty() && normalize_title(&e.title) == title && e.first_author_family().to_lowercase() == family
        })
    }

    /// An entry outside the trash with this DOI.
    pub fn find_by_doi(&self, doi: &str) -> Option<&Entry> {
        let doi = normalize_doi(doi);
        self.entries
            .iter()
            .find(|e| e.trashed.is_none() && normalize_doi(&e.doi) == doi)
    }

    /// A key based on `base` that no other entry outside the trash uses.
    pub fn unique_key(&self, base: &str, title: &str, except_id: &str) -> String {
        let taken: HashSet<&str> = self
            .entries
            .iter()
            .filter(|e| e.id != except_id && e.trashed.is_none())
            .map(|e| e.key.as_str())
            .collect();
        disambiguate_key(base, title, |k| taken.contains(k))
    }

    pub fn add_entry(&mut self, mut entry: Entry, now: f64) -> AddOutcome {
        if let Some(existing) = Some(&entry.doi)
            .filter(|d| !d.trim().is_empty())
            .and_then(|d| self.find_by_doi(d))
        {
            return AddOutcome::Duplicate(existing.id.clone());
        }
        if let Some(existing) = self.find_duplicate(&entry) {
            if entry.doi.trim().is_empty() {
                return AddOutcome::Duplicate(existing.id.clone());
            }
            let (id, old_key, added) = (existing.id.clone(), existing.key.clone(), existing.added);
            entry.key = self.unique_key(&entry.key, &entry.title, &id);
            entry.id = id.clone();
            entry.added = added;
            entry.modified = now;
            entry.trashed = None;
            if let Some(slot) = self.entries.iter_mut().find(|e| e.id == id) {
                *slot = entry;
            }
            return AddOutcome::Replaced { id, old_key };
        }
        entry.key = self.unique_key(&entry.key, &entry.title, &entry.id);
        entry.added = now;
        entry.modified = now;
        let id = entry.id.clone();
        self.entries.push(entry);
        AddOutcome::Added(id)
    }

    pub fn update_entry(&mut self, mut entry: Entry, now: f64) {
        entry.key = self.unique_key(&entry.key, &entry.title, &entry.id);
        entry.modified = now;
        if let Some(slot) = self.entries.iter_mut().find(|e| e.id == entry.id) {
            *slot = entry;
        }
    }

    pub fn trash_entries(&mut self, ids: &[String], now: f64) {
        for e in self
            .entries
            .iter_mut()
            .filter(|e| ids.contains(&e.id) && e.trashed.is_none())
        {
            e.trashed = Some(now);
            e.modified = now;
        }
    }

    /// Restores entries from the trash, renaming keys that are now taken.
    pub fn restore_entries(&mut self, ids: &[String], now: f64) {
        let restored: Vec<String> = self
            .entries
            .iter()
            .filter(|e| ids.contains(&e.id) && e.trashed.is_some())
            .map(|e| e.id.clone())
            .collect();
        for id in restored {
            let Some((key, title)) = self.entry(&id).map(|e| (e.key.clone(), e.title.clone())) else {
                continue;
            };
            let key = self.unique_key(&key, &title, &id);
            if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                e.trashed = None;
                e.key = key;
                e.modified = now;
            }
        }
    }

    /// Permanently deletes entries.
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

    pub fn create_collection(&mut self, id: String, name: &str, parent: Option<&str>, now: f64) {
        let parent = parent.filter(|p| self.collection(p).is_some()).map(str::to_string);
        let order = self
            .collections
            .iter()
            .filter(|c| self.parent_of(c) == parent.as_deref())
            .map(|c| c.order + 1.0)
            .fold(0.0, f64::max);
        self.collections.push(Collection {
            id,
            name: name.trim().to_string(),
            parent,
            entries: vec![],
            order,
            modified: now,
        });
    }

    /// The parent of `c`, ignoring references to missing collections.
    pub fn parent_of<'a>(&self, c: &'a Collection) -> Option<&'a str> {
        c.parent
            .as_deref()
            .filter(|p| *p != c.id && self.collection(p).is_some())
    }

    fn children(&self, parent: Option<&str>) -> Vec<&Collection> {
        let mut children: Vec<&Collection> = self
            .collections
            .iter()
            .filter(|c| self.parent_of(c) == parent)
            .collect();
        children.sort_by(|a, b| a.order.total_cmp(&b.order).then_with(|| a.id.cmp(&b.id)));
        children
    }

    /// All collections in display order with their depth; collections in a parent cycle are top level.
    pub fn tree(&self) -> Vec<(&Collection, usize)> {
        fn walk<'a>(
            lib: &'a Library,
            c: &'a Collection,
            depth: usize,
            seen: &mut HashSet<&'a str>,
            out: &mut Vec<(&'a Collection, usize)>,
        ) {
            if !seen.insert(c.id.as_str()) {
                return;
            }
            out.push((c, depth));
            for child in lib.children(Some(&c.id)) {
                walk(lib, child, depth + 1, seen, out);
            }
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for root in self.children(None) {
            walk(self, root, 0, &mut seen, &mut out);
        }
        let mut stranded: Vec<&Collection> = self
            .collections
            .iter()
            .filter(|c| !seen.contains(c.id.as_str()))
            .collect();
        stranded.sort_by(|a, b| a.id.cmp(&b.id));
        for c in stranded {
            walk(self, c, 0, &mut seen, &mut out);
        }
        out
    }

    /// Ids of all collections below `id`.
    pub fn descendants(&self, id: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut stack = vec![id.to_string()];
        while let Some(current) = stack.pop() {
            for child in self.children(Some(&current)) {
                if child.id != id && !out.contains(&child.id) {
                    out.push(child.id.clone());
                    stack.push(child.id.clone());
                }
            }
        }
        out
    }

    /// `Topic / Subtopic`.
    pub fn collection_path(&self, id: &str) -> String {
        let mut names = Vec::new();
        let mut current = self.collection(id);
        while let Some(c) = current {
            if names.len() > self.collections.len() {
                break;
            }
            names.push(c.name.as_str());
            current = self.parent_of(c).and_then(|p| self.collection(p));
        }
        names.reverse();
        names.join(" / ")
    }

    /// Moves `id` before, after or into `target`. Moving into its own subtree is ignored.
    pub fn move_collection(&mut self, id: &str, target: &str, place: Place, now: f64) {
        if id == target || self.descendants(id).iter().any(|d| d == target) {
            return;
        }
        let Some(target_col) = self.collection(target) else {
            return;
        };
        let parent = match place {
            Place::Inside => Some(target.to_string()),
            _ => self.parent_of(target_col).map(str::to_string),
        };
        let mut siblings: Vec<String> = self
            .children(parent.as_deref())
            .into_iter()
            .map(|c| c.id.clone())
            .filter(|c| c != id)
            .collect();
        let at = match place {
            Place::Inside => siblings.len(),
            Place::Before => siblings.iter().position(|c| c == target).unwrap_or(0),
            Place::After => siblings
                .iter()
                .position(|c| c == target)
                .map_or(siblings.len(), |p| p + 1),
        };
        siblings.insert(at, id.to_string());
        for (i, sid) in siblings.iter().enumerate() {
            if let Some(c) = self.collection_mut(sid) {
                let moved = c.id == id && c.parent != parent;
                if c.order != i as f64 || moved {
                    c.order = i as f64;
                    c.modified = now;
                }
                if c.id == id {
                    c.parent = parent.clone();
                }
            }
        }
    }

    pub fn rename_collection(&mut self, id: &str, name: &str, now: f64) {
        if let Some(c) = self.collection_mut(id) {
            c.name = name.trim().to_string();
            c.modified = now;
        }
    }

    /// Deletes a collection and everything below it; the entries stay in the library.
    pub fn delete_collection(&mut self, id: &str, now: f64) {
        let mut ids = self.descendants(id);
        ids.push(id.to_string());
        self.collections.retain(|c| !ids.contains(&c.id));
        for id in ids {
            self.deleted.insert(id, now);
        }
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

    /// Adds or removes one entry in one collection only.
    pub fn set_membership(&mut self, collection_id: &str, entry_id: &str, member: bool, now: f64) {
        let Some(c) = self.collection_mut(collection_id) else {
            return;
        };
        let present = c.entries.iter().any(|e| e == entry_id);
        if member && !present {
            c.entries.push(entry_id.to_string());
        } else if !member && present {
            c.entries.retain(|e| e != entry_id);
        } else {
            return;
        }
        c.modified = now;
    }

    /// Removes entries from a collection and all collections below it.
    pub fn remove_from_collection(&mut self, collection_id: &str, ids: &[String], now: f64) {
        let mut scope = self.descendants(collection_id);
        scope.push(collection_id.to_string());
        for c in self.collections.iter_mut().filter(|c| scope.contains(&c.id)) {
            let before = c.entries.len();
            c.entries.retain(|id| !ids.contains(id));
            if c.entries.len() != before {
                c.modified = now;
            }
        }
    }

    /// Entries of a view; a collection includes the entries of its subcollections.
    pub fn entries_in(&self, view: &View) -> Vec<&Entry> {
        match view {
            View::Library => self.entries.iter().filter(|e| e.trashed.is_none()).collect(),
            View::Trash => self.entries.iter().filter(|e| e.trashed.is_some()).collect(),
            View::Collection(cid) => {
                if self.collection(cid).is_none() {
                    return vec![];
                }
                let mut scope = vec![cid.clone()];
                scope.extend(self.descendants(cid));
                let mut seen = HashSet::new();
                scope
                    .iter()
                    .filter_map(|c| self.collection(c))
                    .flat_map(|c| c.entries.iter())
                    .filter(|id| seen.insert(id.as_str()))
                    .filter_map(|id| self.entry(id))
                    .filter(|e| e.trashed.is_none())
                    .collect()
            }
        }
    }
}

/// `base`, else `base` plus the first significant title word, else that with `a`, `b`, ... appended.
pub fn disambiguate_key(base: &str, title: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    let stem = format!("{base}{}", crate::bibtex::title_word(title));
    if stem != base && !taken(&stem) {
        return stem;
    }
    (0..)
        .map(|n| format!("{stem}{}", alpha_suffix(n)))
        .find(|k| !taken(k))
        .unwrap()
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
    fn clashing_keys_use_title_word() {
        let mut lib = Library::default();
        let titles = [
            "Deep Residual Learning for Image Recognition",
            "Identity Mappings in Deep Residual Networks",
            "Deep Networks with Stochastic Depth",
            "Deep Pyramid Networks",
            "A",
        ];
        for (i, t) in titles.iter().enumerate() {
            lib.add_entry(entry(&i.to_string(), "", t, "He", "He2016CVPR"), 0.0);
        }
        let keys: Vec<_> = lib.entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "He2016CVPR",
                "He2016CVPRIdentity",
                "He2016CVPRDeep",
                "He2016CVPRDeepa",
                "He2016CVPRa"
            ]
        );
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
        lib.create_collection("c".into(), "C", None, 0.0);
        lib.add_to_collection("c", &["e".into()], 1.0);
        lib.delete_entries(&["e".into()], 2.0);
        assert!(lib.collection("c").unwrap().entries.is_empty());
        assert_eq!(lib.deleted.get("e"), Some(&2.0));
    }

    fn tree_of(lib: &Library) -> Vec<String> {
        lib.tree()
            .into_iter()
            .map(|(c, depth)| format!("{}{}", "-".repeat(depth), c.id))
            .collect()
    }

    #[test]
    fn new_doi_replaces_same_paper() {
        let mut lib = Library::default();
        let mut preprint = entry(
            "a",
            "10.48550/arXiv.1706.03762",
            "Attention Is All You Need",
            "Vaswani",
            "Vaswani2017arXiv",
        );
        preprint.venue = "arXiv".into();
        lib.add_entry(preprint, 1.0);
        lib.create_collection("c".into(), "C", None, 1.0);
        lib.add_to_collection("c", &["a".into()], 1.0);

        let mut published = entry(
            "b",
            "10.5555/3295222.3295349",
            "Attention is all you need",
            "Vaswani",
            "Vaswani2017NeurIPS",
        );
        published.venue = "NeurIPS".into();
        assert_eq!(
            lib.add_entry(published.clone(), 2.0),
            AddOutcome::Replaced {
                id: "a".into(),
                old_key: "Vaswani2017arXiv".into()
            }
        );
        let e = lib.entry("a").unwrap();
        assert_eq!(
            (lib.entries.len(), e.venue.as_str(), e.key.as_str(), e.added),
            (1, "NeurIPS", "Vaswani2017NeurIPS", 1.0)
        );
        assert_eq!(lib.collection("c").unwrap().entries, ["a"]);

        assert_eq!(lib.add_entry(published, 3.0), AddOutcome::Duplicate("a".into()));
        assert_eq!(
            lib.add_entry(entry("c", "", "Attention is all you need", "Vaswani", "K"), 3.0),
            AddOutcome::Duplicate("a".into())
        );
    }

    #[test]
    fn trash_is_ignored_for_duplicates_and_keys() {
        let mut lib = Library::default();
        lib.add_entry(entry("old", "10.1/x", "Paper", "He", "He2016CVPR"), 0.0);
        lib.trash_entries(&["old".into()], 1.0);
        assert!(matches!(
            lib.add_entry(entry("new", "10.1/x", "Paper", "He", "He2016CVPR"), 2.0),
            AddOutcome::Added(_)
        ));
        assert_eq!(lib.entry("new").unwrap().key, "He2016CVPR");
        assert!(lib.find_by_doi("10.1/X").is_some_and(|e| e.id == "new"));

        lib.restore_entries(&["old".into()], 3.0);
        assert_eq!(lib.entry("old").unwrap().key, "He2016CVPRPaper");
    }

    #[test]
    fn trash_and_restore() {
        let mut lib = Library::default();
        lib.add_entry(entry("e", "", "T", "A", "k"), 0.0);
        lib.create_collection("c".into(), "C", None, 0.0);
        lib.add_to_collection("c", &["e".into()], 0.0);
        let count = |lib: &Library, view: View| lib.entries_in(&view).len();

        lib.trash_entries(&["e".into()], 1.0);
        assert_eq!(
            (
                count(&lib, View::Library),
                count(&lib, View::Collection("c".into())),
                count(&lib, View::Trash)
            ),
            (0, 0, 1)
        );
        lib.restore_entries(&["e".into()], 2.0);
        assert_eq!(
            (
                count(&lib, View::Library),
                count(&lib, View::Collection("c".into())),
                count(&lib, View::Trash)
            ),
            (1, 1, 0)
        );
        assert_eq!(lib.entry("e").unwrap().modified, 2.0);
    }

    #[test]
    fn moves_collections() {
        let mut lib = Library::default();
        for id in ["a", "b", "c", "d"] {
            lib.create_collection(id.into(), id, None, 0.0);
        }
        lib.move_collection("d", "a", Place::Before, 1.0);
        assert_eq!(tree_of(&lib), ["d", "a", "b", "c"]);
        lib.move_collection("d", "b", Place::After, 2.0);
        assert_eq!(tree_of(&lib), ["a", "b", "d", "c"]);
        lib.create_collection("e".into(), "e", None, 3.0);
        assert_eq!(tree_of(&lib), ["a", "b", "d", "c", "e"]);
    }

    #[test]
    fn nests_collections() {
        let mut lib = Library::default();
        lib.create_collection("topic".into(), "Topic", None, 0.0);
        lib.create_collection("sub".into(), "Sub", Some("topic"), 0.0);
        lib.create_collection("deep".into(), "Deep", Some("sub"), 0.0);
        lib.create_collection("other".into(), "Other", None, 0.0);
        assert_eq!(tree_of(&lib), ["topic", "-sub", "--deep", "other"]);
        assert_eq!(lib.collection_path("deep"), "Topic / Sub / Deep");

        lib.move_collection("other", "sub", Place::Inside, 1.0);
        assert_eq!(tree_of(&lib), ["topic", "-sub", "--deep", "--other"]);
        lib.move_collection("topic", "deep", Place::Inside, 2.0);
        assert_eq!(
            tree_of(&lib),
            ["topic", "-sub", "--deep", "--other"],
            "cannot move into own subtree"
        );
        lib.move_collection("deep", "topic", Place::Before, 3.0);
        assert_eq!(tree_of(&lib), ["deep", "topic", "-sub", "--other"]);
    }

    #[test]
    fn parent_shows_and_exports_subcollection_entries() {
        let mut lib = Library::default();
        for id in ["e1", "e2", "e3"] {
            lib.add_entry(entry(id, "", id, "A", id), 0.0);
        }
        lib.create_collection("topic".into(), "Topic", None, 0.0);
        lib.create_collection("sub".into(), "Sub", Some("topic"), 0.0);
        lib.add_to_collection("topic", &["e1".into()], 0.0);
        lib.add_to_collection("sub", &["e1".into(), "e2".into()], 0.0);
        let ids = |lib: &Library, cid: &str| -> Vec<String> {
            lib.entries_in(&View::Collection(cid.into()))
                .iter()
                .map(|e| e.id.clone())
                .collect()
        };
        assert_eq!(ids(&lib, "topic"), ["e1", "e2"]);
        assert_eq!(ids(&lib, "sub"), ["e1", "e2"]);

        lib.remove_from_collection("topic", &["e2".into()], 1.0);
        assert_eq!(ids(&lib, "sub"), ["e1"]);

        lib.delete_collection("topic", 2.0);
        assert!(lib.collections.is_empty());
        assert!(lib.deleted.contains_key("sub"));
        assert_eq!(lib.entries.len(), 3);
    }

    #[test]
    fn entries_show_up_in_every_ancestor() {
        let mut lib = Library::default();
        lib.add_entry(entry("e", "", "Deep paper", "A", "k"), 0.0);
        lib.create_collection("topic".into(), "Topic", None, 0.0);
        lib.create_collection("sub".into(), "Sub", Some("topic"), 0.0);
        lib.create_collection("deep".into(), "Deep", Some("sub"), 0.0);
        lib.create_collection("other".into(), "Other", None, 0.0);
        lib.add_to_collection("deep", &["e".into()], 0.0);
        let has = |lib: &Library, cid: &str| !lib.entries_in(&View::Collection(cid.into())).is_empty();
        assert!(has(&lib, "deep") && has(&lib, "sub") && has(&lib, "topic"));
        assert!(!has(&lib, "other"));

        lib.move_collection("deep", "other", Place::Inside, 1.0);
        assert!(has(&lib, "other") && !has(&lib, "topic"));
    }

    #[test]
    fn tree_survives_cycles_and_missing_parents() {
        let mut lib = Library::default();
        lib.create_collection("a".into(), "A", None, 0.0);
        lib.create_collection("b".into(), "B", Some("a"), 0.0);
        lib.collection_mut("a").unwrap().parent = Some("b".into());
        lib.create_collection("c".into(), "C", None, 0.0);
        lib.collection_mut("c").unwrap().parent = Some("gone".into());
        assert_eq!(tree_of(&lib), ["c", "a", "-b"]);
        assert_eq!(lib.descendants("a"), ["b"]);
    }
}
