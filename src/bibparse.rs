//! Parses pasted BibTeX (Google Scholar, ACM, IEEE, DBLP, arXiv, ...) into entries.

use crate::bibtex::{command_char, compose_accent};
use crate::doi;
use crate::metadata::{is_acronym, publisher_address, venue_abbrev};
use crate::model::{Entry, EntryType, Person};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct RawEntry {
    pub kind: String,
    pub key: String,
    /// Lowercase field names with raw LaTeX values.
    pub fields: Vec<(String, String)>,
}

/// True if the text contains something like `@article{`.
pub fn looks_like_bibtex(input: &str) -> bool {
    let b = input.as_bytes();
    let mut from = 0;
    while let Some(off) = input[from..].find('@') {
        let mut i = from + off + 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_alphabetic() {
            i += 1;
        }
        while i > start && i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i > start && matches!(b.get(i), Some(b'{') | Some(b'(')) {
            return true;
        }
        from = from + off + 1;
    }
    false
}

/// Returns the parsed entries and the line numbers of entries that could not be parsed.
pub fn parse(input: &str) -> (Vec<RawEntry>, Vec<usize>) {
    let mut p = Parser {
        s: input,
        b: input.as_bytes(),
        pos: 0,
        macros: HashMap::new(),
    };
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    while let Some(off) = input[p.pos..].find('@') {
        let at = p.pos + off;
        p.pos = at;
        match p.entry() {
            Some(Some(e)) => entries.push(e),
            Some(None) => {}
            None => {
                errors.push(input[..at].matches('\n').count() + 1);
                p.pos = at + 1;
            }
        }
    }
    (entries, errors)
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    pos: usize,
    macros: HashMap<String, String>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn ident(&mut self) -> &str {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || b"_-:.+/".contains(&c))
        {
            self.pos += 1;
        }
        &self.s[start..self.pos]
    }

    /// Content of a `{...}` group starting at the current position.
    fn braced(&mut self) -> Option<&str> {
        let start = self.pos + 1;
        let mut depth = 0;
        while let Some(c) = self.peek() {
            match c {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.pos += 1;
                        return Some(&self.s[start..self.pos - 1]);
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        None
    }

    fn quoted(&mut self) -> Option<&str> {
        let start = self.pos + 1;
        self.pos += 1;
        let mut depth = 0;
        while let Some(c) = self.peek() {
            match c {
                b'{' => depth += 1,
                b'}' => depth -= 1,
                b'"' if depth == 0 => {
                    self.pos += 1;
                    return Some(&self.s[start..self.pos - 1]);
                }
                _ => {}
            }
            self.pos += 1;
        }
        None
    }

    /// A field value, possibly several parts joined with `#`.
    fn value(&mut self) -> Option<String> {
        let mut out = String::new();
        loop {
            self.skip_ws();
            match self.peek()? {
                b'{' => out.push_str(self.braced()?),
                b'"' => out.push_str(self.quoted()?),
                c if c.is_ascii_alphanumeric() => {
                    let name = self.ident().to_string();
                    let lower = name.to_lowercase();
                    out.push_str(self.macros.get(&lower).map(String::as_str).unwrap_or(&name));
                }
                _ => return None,
            }
            self.skip_ws();
            if self.peek() != Some(b'#') {
                return Some(out);
            }
            self.pos += 1;
        }
    }

    /// `None` on a syntax error, `Some(None)` for `@string`, `@comment` and `@preamble`.
    fn entry(&mut self) -> Option<Option<RawEntry>> {
        self.pos += 1;
        let kind = self.ident().to_lowercase();
        self.skip_ws();
        let close = match self.peek()? {
            b'{' => b'}',
            b'(' => b')',
            _ => return None,
        };
        if kind == "comment" {
            if close == b'}' {
                self.braced()?;
            } else {
                self.pos += self.s[self.pos..].find(')')? + 1;
            }
            return Some(None);
        }
        self.pos += 1;
        self.skip_ws();
        if kind == "preamble" {
            self.value()?;
            self.skip_ws();
            return (self.peek()? == close).then(|| {
                self.pos += 1;
                None
            });
        }
        if kind == "string" {
            let name = self.ident().to_lowercase();
            self.skip_ws();
            if self.peek()? != b'=' {
                return None;
            }
            self.pos += 1;
            let value = self.value()?;
            self.macros.insert(name, value);
            self.skip_ws();
            return (self.peek()? == close).then(|| {
                self.pos += 1;
                None
            });
        }

        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c != b',' && c != close && !c.is_ascii_whitespace())
        {
            self.pos += 1;
        }
        let key = self.s[start..self.pos].to_string();
        let mut fields = Vec::new();
        loop {
            self.skip_ws();
            match self.peek()? {
                c if c == close => {
                    self.pos += 1;
                    break;
                }
                b',' => {
                    self.pos += 1;
                    continue;
                }
                _ => {}
            }
            let name = self.ident().to_lowercase();
            if name.is_empty() {
                return None;
            }
            self.skip_ws();
            if self.peek()? != b'=' {
                return None;
            }
            self.pos += 1;
            let value = self.value()?;
            fields.push((name, value));
        }
        Some(Some(RawEntry { kind, key, fields }))
    }
}

/// Converts LaTeX markup to plain Unicode text.
pub fn delatex(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\\' => {
                i = command(&chars, i, &mut out);
                continue;
            }
            '{' | '}' | '$' => {}
            '~' => out.push(' '),
            '-' if next == Some('-') => {
                if chars.get(i + 2) == Some(&'-') {
                    out.push('—');
                    i += 1;
                } else {
                    out.push('–');
                }
                i += 1;
            }
            '`' if next == Some('`') => {
                out.push('“');
                i += 1;
            }
            '\'' if next == Some('\'') => {
                out.push('”');
                i += 1;
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

const LETTER_ACCENTS: [&str; 9] = ["c", "v", "r", "H", "k", "u", "d", "b", "t"];

/// Handles a backslash at `chars[i]`; returns the index after it.
fn command(chars: &[char], i: usize, out: &mut String) -> usize {
    let Some(&n) = chars.get(i + 1) else { return i + 1 };
    if "'`^\"~=.".contains(n) {
        let (base, next) = accent_arg(chars, i + 2);
        push_accented(out, &n.to_string(), base);
        return next;
    }
    if !n.is_ascii_alphabetic() {
        match n {
            '\\' | ',' | ';' | ' ' | '/' => out.push(' '),
            _ => out.push(n),
        }
        return i + 2;
    }
    let mut j = i + 1;
    while chars.get(j).is_some_and(char::is_ascii_alphabetic) {
        j += 1;
    }
    let name: String = chars[i + 1..j].iter().collect();
    while chars.get(j).is_some_and(|c| *c == ' ') {
        j += 1;
    }
    if LETTER_ACCENTS.contains(&name.as_str()) {
        let (base, next) = accent_arg(chars, j);
        push_accented(out, &name, base);
        return next;
    }
    let text = match name.as_str() {
        "aa" => "å",
        "AA" => "Å",
        "i" => "i",
        "j" => "j",
        "ldots" | "dots" | "textellipsis" => "…",
        "textendash" => "–",
        "textemdash" => "—",
        "textasciitilde" => "~",
        "textasciicircum" => "^",
        "textbackslash" => "\\",
        "textless" => "<",
        "textgreater" => ">",
        "textbar" => "|",
        "textquoteright" => "’",
        "textquoteleft" => "‘",
        "S" => "§",
        _ => {
            if let Some(c) = command_char(&name) {
                out.push(c);
            }
            return j;
        }
    };
    out.push_str(text);
    j
}

/// The letter an accent applies to: `{e}`, `e`, `{\i}` or `\i`.
fn accent_arg(chars: &[char], k: usize) -> (Option<char>, usize) {
    match chars.get(k) {
        Some('{') => {
            let end = chars[k..].iter().position(|c| *c == '}').map_or(chars.len(), |p| k + p);
            let inner: String = chars[k + 1..end].iter().collect();
            let base = match inner.trim() {
                "\\i" => Some('i'),
                "\\j" => Some('j'),
                s => s.chars().find(|c| c.is_alphabetic()),
            };
            (base, (end + 1).min(chars.len()))
        }
        Some('\\') if matches!(chars.get(k + 1), Some('i') | Some('j')) => {
            let mut next = k + 2;
            while chars.get(next) == Some(&' ') {
                next += 1;
            }
            (chars.get(k + 1).copied(), next)
        }
        Some(c) => (Some(*c), k + 1),
        None => (None, k),
    }
}

fn push_accented(out: &mut String, command: &str, base: Option<char>) {
    if let Some(b) = base {
        out.push(compose_accent(command, b).unwrap_or(b));
    }
}

/// Splits `A and B and C` at brace depth zero.
fn split_names(raw: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = Vec::new();
    for word in depth_zero_words(raw) {
        if word.eq_ignore_ascii_case("and") {
            names.push(current.join(" "));
            current.clear();
        } else {
            current.push(word);
        }
    }
    names.push(current.join(" "));
    names.into_iter().filter(|n| !n.trim().is_empty()).collect()
}

/// Whitespace-separated words, keeping brace groups intact.
fn depth_zero_words(raw: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut depth = 0;
    for c in raw.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if c.is_whitespace() && depth == 0 {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn split_depth_zero(raw: &str, sep: char) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut depth = 0;
    for c in raw.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if c == sep && depth == 0 {
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push(c);
        }
    }
    parts.into_iter().map(|p| p.trim().to_string()).collect()
}

fn is_single_group(raw: &str) -> bool {
    let Some(inner) = raw.strip_prefix('{').and_then(|r| r.strip_suffix('}')) else {
        return false;
    };
    let mut depth = 0;
    for c in inner.chars() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => return false,
            '}' => depth -= 1,
            _ => {}
        }
    }
    true
}

/// Parses one BibTeX name: `Last, First`, `Last, Jr, First` or `First von Last`.
pub fn parse_name(raw: &str) -> Option<Person> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("others") || raw.is_empty() {
        return None;
    }
    if is_single_group(raw) {
        return Some(Person {
            family: delatex(raw),
            literal: true,
            ..Default::default()
        });
    }
    let parts = split_depth_zero(raw, ',');
    let person = match parts.as_slice() {
        [whole] => {
            let words = depth_zero_words(whole);
            let starts_lower = |w: &String| {
                delatex(w)
                    .chars()
                    .find(|c| c.is_alphabetic())
                    .is_some_and(char::is_lowercase)
            };
            let von = (1..words.len().saturating_sub(1)).find(|&i| starts_lower(&words[i]));
            let split = von.unwrap_or(words.len().saturating_sub(1));
            Person {
                family: delatex(&words[split..].join(" ")),
                given: delatex(&words[..split].join(" ")),
                ..Default::default()
            }
        }
        [family, given] => Person {
            family: delatex(family),
            given: delatex(given),
            ..Default::default()
        },
        [family, suffix, given, ..] => Person {
            family: delatex(family),
            suffix: delatex(suffix),
            given: delatex(given),
            literal: false,
        },
        [] => return None,
    };
    (!person.family.is_empty()).then_some(person)
}

fn parse_month(raw: &str) -> Option<u8> {
    let s = delatex(raw).to_lowercase();
    if let Ok(n) = s.trim().parse::<u8>() {
        return (1..=12).contains(&n).then_some(n);
    }
    const NAMES: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    NAMES.iter().position(|m| s.starts_with(m)).map(|p| p as u8 + 1)
}

fn first_year(s: &str) -> Option<i32> {
    s.as_bytes()
        .windows(4)
        .position(|w| w.iter().all(u8::is_ascii_digit))
        .and_then(|p| s[p..p + 4].parse().ok())
}

/// Strips braces and LaTeX escapes from identifiers and URLs without touching `~`.
fn plain(raw: &str) -> String {
    let mut out = raw.replace(['{', '}'], "");
    for esc in ["\\_", "\\%", "\\#", "\\&", "\\$"] {
        out = out.replace(esc, &esc[1..]);
    }
    out.trim().trim_start_matches("\\url").trim().to_string()
}

/// Finds an arXiv id in text such as `arXiv preprint arXiv:1706.03762`.
fn find_arxiv(text: &str) -> Option<String> {
    text.split_whitespace()
        .filter_map(doi::parse_token)
        .find_map(|d| doi::arxiv_id_from_doi(&d).map(str::to_string))
}

pub fn to_entry(raw: &RawEntry) -> Entry {
    let field = |name: &str| {
        raw.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.trim().is_empty())
    };
    let text = |name: &str| field(name).map(delatex).unwrap_or_default();
    let first_text = |names: &[&str]| {
        names
            .iter()
            .map(|n| text(n))
            .find(|v| !v.is_empty())
            .unwrap_or_default()
    };

    let mut kind = match raw.kind.as_str() {
        "article" => EntryType::Article,
        "inproceedings" | "conference" => EntryType::InProceedings,
        "incollection" | "inbook" => EntryType::InCollection,
        "book" => EntryType::Book,
        "phdthesis" => EntryType::PhdThesis,
        "techreport" | "report" => EntryType::TechReport,
        _ => EntryType::Misc,
    };

    let authors: Vec<Person> = field("author")
        .or_else(|| field("editor"))
        .map(|a| split_names(a).iter().filter_map(|n| parse_name(n)).collect())
        .unwrap_or_default();
    let year = field("year").or_else(|| field("date")).and_then(first_year);
    let month = field("month").and_then(parse_month);

    let mut venue = first_text(&["journal", "journaltitle", "booktitle"]);
    let mut url = field("url").map(plain).unwrap_or_default();
    let how = text("howpublished");
    if venue.is_empty() && !how.to_lowercase().starts_with("http") {
        venue = how.clone();
    } else if url.is_empty() && how.starts_with("http") {
        url = how;
    }

    let archive = first_text(&["archiveprefix", "eprinttype"]).to_lowercase();
    let mut eprint = field("eprint").map(plain).unwrap_or_default();
    if eprint.is_empty() {
        eprint = find_arxiv(&venue)
            .or_else(|| find_arxiv(&text("note")))
            .unwrap_or_default();
    }
    let mut doi = field("doi").map(plain).unwrap_or_default();
    if let Some(d) = doi::parse_token(&doi) {
        doi = d;
    }
    if url.contains("doi.org/") {
        if doi.is_empty() {
            doi = doi::parse_token(&url).unwrap_or_default();
        }
        url.clear();
    }

    let lower_venue = venue.to_lowercase();
    let preprint = !eprint.is_empty()
        && (archive.is_empty() || archive == "arxiv")
        && (kind == EntryType::Misc || venue.is_empty() || lower_venue.starts_with("arxiv") || lower_venue == "corr");
    let mut volume = text("volume");
    let mut primary_class = text("primaryclass");
    if preprint {
        kind = EntryType::Misc;
        venue = "arXiv".into();
        volume.clear();
        if doi.is_empty() {
            doi = format!("10.48550/arXiv.{eprint}");
        }
        if url.is_empty() {
            url = format!("https://arxiv.org/abs/{eprint}");
        }
    } else if archive != "arxiv" && !archive.is_empty() {
        eprint.clear();
        primary_class.clear();
    }

    let series = text("series");
    let publisher = first_text(&["publisher", "school", "institution", "organization"]);
    let mut address = text("address");
    if address.is_empty() && !matches!(kind, EntryType::Article | EntryType::Misc) {
        address = publisher_address(&publisher).to_string();
    }
    let series_acronym = series
        .split([' ', '\''])
        .next()
        .filter(|w| is_acronym(w))
        .map(|_| series.as_str());
    let venue_short = if preprint {
        "arXiv".to_string()
    } else {
        venue_abbrev(&venue, series_acronym, &doi)
    };

    Entry {
        kind,
        title: text("title"),
        authors,
        year,
        month,
        venue,
        venue_short,
        series,
        volume,
        number: first_text(&["number", "issue"]),
        pages: text("pages").replace(['–', '—'], "-"),
        publisher,
        address,
        doi,
        url,
        eprint,
        primary_class,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bibtex::cite_key;

    fn entries(input: &str) -> Vec<Entry> {
        let (raw, errors) = parse(input);
        assert!(errors.is_empty(), "{errors:?}");
        raw.iter()
            .map(|r| {
                let mut e = to_entry(r);
                e.key = cite_key(&e);
                e
            })
            .collect()
    }

    const SCHOLAR: &str = r#"
@inproceedings{he2016deep,
  title={Deep residual learning for image recognition},
  author={He, Kaiming and Zhang, Xiangyu and Ren, Shaoqing and Sun, Jian},
  booktitle={Proceedings of the IEEE conference on computer vision and pattern recognition},
  pages={770--778},
  year={2016}
}

@article{vaswani2017attention,
  title={Attention is all you need},
  author={Vaswani, Ashish and Shazeer, Noam and Parmar, Niki and Uszkoreit, Jakob and Jones, Llion and Gomez, Aidan N and Kaiser, {\L}ukasz and Polosukhin, Illia},
  journal={Advances in neural information processing systems},
  volume={30},
  year={2017}
}

@article{achiam2023gpt,
  title={Gpt-4 technical report},
  author={Achiam, Josh and Adler, Steven and others},
  journal={arXiv preprint arXiv:2303.08774},
  year={2023}
}
"#;

    #[test]
    fn google_scholar() {
        let e = entries(SCHOLAR);
        assert_eq!(e.len(), 3);
        assert_eq!(
            (e[0].kind, e[0].key.as_str(), e[0].pages.as_str()),
            (EntryType::InProceedings, "He2016CVPR", "770-778")
        );
        assert_eq!(e[0].authors.len(), 4);
        assert_eq!(e[1].key, "Vaswani2017NeurIPS");
        assert_eq!(
            e[1].authors[6],
            Person {
                family: "Kaiser".into(),
                given: "Łukasz".into(),
                ..Default::default()
            }
        );
        assert_eq!(
            (e[2].kind, e[2].eprint.as_str(), e[2].key.as_str()),
            (EntryType::Misc, "2303.08774", "Achiam2023arXiv")
        );
        assert_eq!(e[2].doi, "10.48550/arXiv.2303.08774");
        assert_eq!(e[2].authors.len(), 2);
    }

    #[test]
    fn acm_export() {
        let e = &entries(
            r#"@inproceedings{10.1145/3290605.3300857,
author = {Roussou, Maria and Perry, Sara and Katifori, Akrivi},
title = {Transformation through Provocation?},
year = {2019},
isbn = {9781450359702},
publisher = {Association for Computing Machinery},
address = {New York, NY, USA},
url = {https://doi.org/10.1145/3290605.3300857},
doi = {10.1145/3290605.3300857},
abstract = {We report on a study.},
booktitle = {Proceedings of the 2019 CHI Conference on Human Factors in Computing Systems},
pages = {1–13},
numpages = {13},
location = {Glasgow, Scotland Uk},
series = {CHI '19}
}"#,
        )[0];
        assert_eq!(
            (e.key.as_str(), e.pages.as_str(), e.url.as_str()),
            ("Roussou2019CHI", "1-13", "")
        );
        assert_eq!(
            (e.address.as_str(), e.doi.as_str()),
            ("New York, NY, USA", "10.1145/3290605.3300857")
        );
    }

    #[test]
    fn ieee_export() {
        let e = &entries(
            "@INPROCEEDINGS{7780459,\n  author={He, Kaiming and Zhang, Xiangyu},\n  booktitle={2016 IEEE Conference on Computer Vision and Pattern Recognition (CVPR)}, \n  title={Deep Residual Learning for Image Recognition}, \n  year={2016},\n  volume={},\n  number={},\n  pages={770-778},\n  keywords={Training;Degradation},\n  doi={10.1109/CVPR.2016.90}}\n",
        )[0];
        assert_eq!(
            (e.key.as_str(), e.volume.as_str(), e.doi.as_str()),
            ("He2016CVPR", "", "10.1109/CVPR.2016.90")
        );
        assert_eq!(e.address, "");
    }

    #[test]
    fn dblp_export() {
        let e = &entries(
            r#"@inproceedings{DBLP:conf/cvpr/HeZRS16,
  author       = {Kaiming He and
                  Xiangyu Zhang and
                  Shaoqing Ren and
                  Jian Sun},
  title        = {Deep Residual Learning for Image Recognition},
  booktitle    = {2016 {IEEE} Conference on Computer Vision and Pattern Recognition,
                  {CVPR} 2016, Las Vegas, NV, USA, June 27-30, 2016},
  pages        = {770--778},
  publisher    = {{IEEE} Computer Society},
  year         = {2016},
  url          = {https://doi.org/10.1109/CVPR.2016.90},
  doi          = {10.1109/CVPR.2016.90},
  biburl       = {https://dblp.org/rec/conf/cvpr/HeZRS16.bib}
}
@article{DBLP:journals/corr/VaswaniSPUJGKP17,
  author       = {Ashish Vaswani and Noam Shazeer},
  title        = {Attention Is All You Need},
  journal      = {CoRR},
  volume       = {abs/1706.03762},
  year         = {2017},
  eprinttype    = {arXiv},
  eprint       = {1706.03762}
}"#,
        );
        assert_eq!(
            e[0].authors[3],
            Person {
                family: "Sun".into(),
                given: "Jian".into(),
                ..Default::default()
            }
        );
        assert_eq!(
            (e[0].key.as_str(), e[0].publisher.as_str(), e[0].url.as_str()),
            ("He2016CVPR", "IEEE Computer Society", "")
        );
        assert_eq!(e[0].address, "Los Alamitos, CA, USA");
        assert_eq!(
            (e[1].kind, e[1].key.as_str(), e[1].volume.as_str()),
            (EntryType::Misc, "Vaswani2017arXiv", "")
        );
    }

    #[test]
    fn arxiv_export() {
        let e = &entries(
            r#"@misc{vaswani2023attention,
      title={Attention Is All You Need},
      author={Ashish Vaswani and Noam Shazeer and Niki Parmar},
      year={2023},
      eprint={1706.03762},
      archivePrefix={arXiv},
      primaryClass={cs.CL},
      url={https://arxiv.org/abs/1706.03762},
}"#,
        )[0];
        assert_eq!(
            (e.kind, e.key.as_str(), e.primary_class.as_str()),
            (EntryType::Misc, "Vaswani2023arXiv", "cs.CL")
        );
        assert_eq!(e.doi, "10.48550/arXiv.1706.03762");
    }

    #[test]
    fn syntax_features() {
        let input = r#"
            @comment{ ignored @article{not, title={x}} }
            @string{acm = "Association for Computing Machinery"}
            @preamble{"\newcommand{\noop}[1]{}"}
            @Article(knuth84,
              Author = "Donald E. Knuth",
              Title = "Literate {P}rogramming",
              Journal = "The Computer Journal",
              Publisher = acm # ", Inc.",
              Year = 1984, Month = may, Volume = 27,
            )
        "#;
        let e = &entries(input)[0];
        assert_eq!(e.title, "Literate Programming");
        assert_eq!(
            e.authors[0],
            Person {
                family: "Knuth".into(),
                given: "Donald E.".into(),
                ..Default::default()
            }
        );
        assert_eq!(e.publisher, "Association for Computing Machinery, Inc.");
        assert_eq!((e.year, e.month, e.volume.as_str()), (Some(1984), Some(5), "27"));
    }

    #[test]
    fn reports_broken_entries_and_keeps_going() {
        let (raw, errors) = parse("@article{ok, title={A}}\n\n@article{bad, title = {unclosed\n@misc{fine, title={B}}");
        assert_eq!(raw.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["ok", "fine"]);
        assert_eq!(errors, [3]);
    }

    #[test]
    fn converts_latex() {
        let cases = [
            (r#"G{\"o}del"#, "Gödel"),
            (r"{\'E}cole Fran\c{c}aise", "École Française"),
            (r"\v{S}koda {\L}ukasz Dvo\v{r}\'{a}k", "Škoda Łukasz Dvořák"),
            (r#"na\"{\i}ve na\"\i ve"#, "naïve naïve"),
            (r"\textit{in vivo} 50\% R\&D", "in vivo 50% R&D"),
            (r"$\alpha$-tests ``quoted'' 1--2", "α-tests “quoted” 1–2"),
            (r#"Gro{\ss}e {\aa}ngstr{\"o}m"#, "Große ångström"),
        ];
        for (tex, want) in cases {
            assert_eq!(delatex(tex), want, "{tex}");
        }
    }

    #[test]
    fn parses_names() {
        let name = |s: &str| parse_name(s).unwrap().to_line();
        assert_eq!(name("van der Maaten, Laurens"), "van der Maaten, Laurens");
        assert_eq!(name("Ludwig van Beethoven"), "van Beethoven, Ludwig");
        assert_eq!(name("King, Jr., Martin Luther"), "King, Jr., Martin Luther");
        assert_eq!(name("{OpenAI}"), "{OpenAI}");
        assert_eq!(name("{\\'E}mile Zola"), "Zola, Émile");
        assert_eq!(name("Plato"), "Plato");
        assert!(parse_name("others").is_none());
        assert_eq!(split_names("A and {Smith and Sons} and B").len(), 3);
    }

    #[test]
    fn detects_bibtex() {
        assert!(looks_like_bibtex("  @article{x,"));
        assert!(looks_like_bibtex("@misc (x,"));
        assert!(!looks_like_bibtex("10.1145/3290605.3300857"));
        assert!(!looks_like_bibtex("mail me at a@b.com"));
    }
}
