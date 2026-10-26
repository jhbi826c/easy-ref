use crate::model::{Entry, EntryType, Person};
use std::collections::HashMap;

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// `{FirstAuthorFamily}{Year}{Venue}`, e.g. `He2016CVPR`.
pub fn cite_key(e: &Entry) -> String {
    let name = match e.authors.first() {
        Some(p) => camel_ascii(&p.family),
        None => camel_ascii(e.title.split_whitespace().next().unwrap_or("Anon")),
    };
    let year = e.year.map(|y| y.to_string()).unwrap_or_default();
    let venue: String = ascii_fold(&e.venue_short)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    format!("{name}{year}{venue}")
}

/// Keeps only characters that are safe in a citation key.
pub fn sanitize_key(key: &str) -> String {
    ascii_fold(key)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_:./+".contains(*c))
        .collect()
}

fn camel_ascii(s: &str) -> String {
    ascii_fold(s)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            let first = c.next().unwrap().to_ascii_uppercase();
            std::iter::once(first).chain(c).collect::<String>()
        })
        .collect()
}

pub fn to_bibtex(entries: &[&Entry]) -> String {
    entries
        .iter()
        .map(|e| entry_to_bibtex(e))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn entry_to_bibtex(e: &Entry) -> String {
    let mut fields: Vec<(&str, String)> = Vec::new();
    let mut put = |name: &'static str, value: String| {
        if !value.is_empty() {
            fields.push((name, value));
        }
    };
    let braced = |s: &str| {
        if s.trim().is_empty() {
            String::new()
        } else {
            format!("{{{}}}", latex_text(s.trim()))
        }
    };
    let raw = |s: &str| {
        if s.trim().is_empty() {
            String::new()
        } else {
            format!("{{{}}}", s.trim())
        }
    };

    put(
        "author",
        if e.authors.is_empty() {
            String::new()
        } else {
            format!("{{{}}}", format_authors(&e.authors))
        },
    );
    put(
        "title",
        if e.title.trim().is_empty() {
            String::new()
        } else {
            format!("{{{}}}", latex_title(e.title.trim()))
        },
    );

    match e.kind {
        EntryType::Article => {
            put("journal", braced(&e.venue));
            put("volume", braced(&e.volume));
            put("number", braced(&e.number));
        }
        EntryType::InProceedings | EntryType::InCollection => {
            put("booktitle", braced(&e.venue));
            put("series", braced(&e.series));
            put("volume", braced(&e.volume));
            if !e.series.is_empty() && e.volume.is_empty() {
                put("number", braced(&e.number));
            }
        }
        EntryType::Book => {
            put("series", braced(&e.series));
            put("volume", braced(&e.volume));
        }
        EntryType::TechReport => put("number", braced(&e.number)),
        EntryType::PhdThesis => {}
        EntryType::Misc => {
            let how = if e.eprint.is_empty() {
                braced(&e.venue)
            } else {
                format!("{{arXiv preprint arXiv:{}}}", e.eprint)
            };
            put("howpublished", how);
        }
    }

    put("pages", braced(&page_range(&e.pages)));
    put("year", e.year.map(|y| format!("{{{y}}}")).unwrap_or_default());
    let month = e.month.and_then(|m| MONTHS.get((m as usize).checked_sub(1)?));
    put("month", month.map(|m| m.to_string()).unwrap_or_default());

    let publisher_field = match e.kind {
        EntryType::PhdThesis => "school",
        EntryType::TechReport => "institution",
        EntryType::Misc => "",
        _ => "publisher",
    };
    if !publisher_field.is_empty() {
        put(publisher_field, braced(&e.publisher));
        if e.kind != EntryType::Article {
            put("address", braced(&e.address));
        }
    }

    if !e.eprint.is_empty() {
        put("eprint", raw(&e.eprint));
        put("archiveprefix", "{arXiv}".into());
        put("primaryclass", raw(&e.primary_class));
    }
    put("doi", raw(&e.doi));
    if e.doi.is_empty() || !e.eprint.is_empty() {
        put("url", raw(&e.url));
    }
    if e.authors.is_empty() {
        put("key", braced(&e.key));
    }

    let width = fields.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    let body: Vec<String> = fields.iter().map(|(n, v)| format!("  {n:<width$} = {v}")).collect();
    format!("@{}{{{},\n{}\n}}\n", e.kind.bibtex_name(), e.key, body.join(",\n"))
}

fn format_authors(authors: &[Person]) -> String {
    authors
        .iter()
        .map(|p| {
            let part = |s: &str| protect_and(&latex_text(&s.replace(',', " ")));
            if p.literal {
                format!("{{{}}}", latex_text(&p.family))
            } else if p.given.is_empty() {
                format!("{{{}}}", latex_text(&p.family))
            } else if p.suffix.is_empty() {
                format!("{}, {}", part(&p.family), part(&p.given))
            } else {
                format!("{}, {}, {}", part(&p.family), part(&p.suffix), part(&p.given))
            }
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

fn protect_and(s: &str) -> String {
    if s.split_whitespace().any(|w| w.eq_ignore_ascii_case("and")) {
        format!("{{{s}}}")
    } else {
        s.to_string()
    }
}

fn page_range(pages: &str) -> String {
    let p = pages.trim().replace(['–', '—'], "-");
    if p.contains("--") {
        return p;
    }
    match p.split_once('-') {
        Some((a, b)) if !a.trim().is_empty() && !b.trim().is_empty() => format!("{}--{}", a.trim(), b.trim()),
        _ => p,
    }
}

/// Escapes the title and braces words whose capitalization must survive sentence-casing styles.
fn latex_title(title: &str) -> String {
    title
        .split_whitespace()
        .map(|token| {
            let start = token.find(|c: char| c.is_alphanumeric());
            let end = token
                .rfind(|c: char| c.is_alphanumeric())
                .map(|i| i + token[i..].chars().next().unwrap().len_utf8());
            let (Some(start), Some(end)) = (start, end) else {
                return latex_text(token);
            };
            let (pre, core, post) = (&token[..start], &token[start..end], &token[end..]);
            let core_tex = latex_text(core);
            let protect = core.split('-').any(|part| part.chars().skip(1).any(char::is_uppercase));
            if protect {
                format!("{}{{{}}}{}", latex_text(pre), core_tex, latex_text(post))
            } else {
                format!("{}{}{}", latex_text(pre), core_tex, latex_text(post))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escapes LaTeX special characters and converts accented letters to LaTeX commands.
pub fn latex_text(s: &str) -> String {
    let table = accent_table();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' | '%' | '$' | '#' | '_' => {
                out.push('\\');
                out.push(c);
            }
            '{' | '}' => {}
            '\\' => out.push_str("\\textbackslash{}"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            '<' => out.push_str("$<$"),
            '>' => out.push_str("$>$"),
            '|' => out.push_str("$|$"),
            '–' => out.push_str("--"),
            '—' => out.push_str("---"),
            '‘' => out.push('`'),
            '’' => out.push('\''),
            '“' => out.push_str("``"),
            '”' => out.push_str("''"),
            '…' => out.push_str("\\ldots{}"),
            '\u{a0}' => out.push('~'),
            _ if c.is_ascii() => out.push(c),
            _ => match table.get(&c) {
                Some((tex, _)) => out.push_str(tex),
                None => match math_symbol(c) {
                    Some(m) => {
                        out.push('$');
                        out.push_str(m);
                        out.push('$');
                    }
                    None => out.push(c),
                },
            },
        }
    }
    out
}

pub fn ascii_fold(s: &str) -> String {
    let table = accent_table();
    s.chars()
        .map(|c| match table.get(&c) {
            Some((_, ascii)) => ascii.clone(),
            None => c.to_string(),
        })
        .collect()
}

const ACCENTS: [(&str, &str, &str); 13] = [
    ("'", "áéíóúýÁÉÍÓÚÝćĆńŃśŚźŹĺĹŕŔ", "aeiouyAEIOUYcCnNsSzZlLrR"),
    ("`", "àèìòùÀÈÌÒÙ", "aeiouAEIOU"),
    ("^", "âêîôûÂÊÎÔÛĉĈĝĜĥĤĵĴŝŜŵŴŷŶ", "aeiouAEIOUcCgGhHjJsSwWyY"),
    ("\"", "äëïöüÿÄËÏÖÜŸ", "aeiouyAEIOUY"),
    ("~", "ãñõÃÑÕĩĨũŨ", "anoANOiIuU"),
    ("c", "çÇşŞţŢ", "cCsStT"),
    ("v", "čČšŠžŽřŘěĚďĎťŤňŇ", "cCsSzZrReEdDtTnN"),
    ("r", "åÅůŮ", "aAuU"),
    ("H", "őŐűŰ", "oOuU"),
    ("k", "ąĄęĘ", "aAeE"),
    (".", "żŻėĖ", "zZeE"),
    ("u", "ăĂğĞ", "aAgG"),
    ("=", "āĀēĒīĪōŌūŪ", "aAeEiIoOuU"),
];

const LETTERS: [(char, &str, &str); 13] = [
    ('ß', "{\\ss}", "ss"),
    ('ø', "{\\o}", "o"),
    ('Ø', "{\\O}", "O"),
    ('æ', "{\\ae}", "ae"),
    ('Æ', "{\\AE}", "AE"),
    ('œ', "{\\oe}", "oe"),
    ('Œ', "{\\OE}", "OE"),
    ('ł', "{\\l}", "l"),
    ('Ł', "{\\L}", "L"),
    ('ı', "{\\i}", "i"),
    ('đ', "d", "d"),
    ('Đ', "D", "D"),
    ('ð', "d", "d"),
];

/// Maps a character to `(LaTeX, ASCII)`.
fn accent_table() -> &'static HashMap<char, (String, String)> {
    use std::sync::OnceLock;
    static TABLE: OnceLock<HashMap<char, (String, String)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut m = HashMap::new();
        for (cmd, chars, bases) in ACCENTS {
            for (c, base) in chars.chars().zip(bases.chars()) {
                let tex = if cmd.chars().all(char::is_alphabetic) {
                    format!("{{\\{cmd}{{{base}}}}}")
                } else {
                    format!("{{\\{cmd}{base}}}")
                };
                m.insert(c, (tex, base.to_string()));
            }
        }
        for (c, tex, ascii) in LETTERS {
            m.insert(c, (tex.to_string(), ascii.to_string()));
        }
        m
    })
}

fn math_symbol(c: char) -> Option<&'static str> {
    Some(match c {
        'α' => "\\alpha",
        'β' => "\\beta",
        'γ' => "\\gamma",
        'δ' => "\\delta",
        'ε' => "\\epsilon",
        'θ' => "\\theta",
        'λ' => "\\lambda",
        'μ' => "\\mu",
        'π' => "\\pi",
        'σ' => "\\sigma",
        'τ' => "\\tau",
        'φ' => "\\phi",
        'ω' => "\\omega",
        'Δ' => "\\Delta",
        'Σ' => "\\Sigma",
        'Ω' => "\\Omega",
        '×' => "\\times",
        '±' => "\\pm",
        '≤' => "\\leq",
        '≥' => "\\geq",
        '≈' => "\\approx",
        '→' => "\\rightarrow",
        '∞' => "\\infty",
        '°' => "^\\circ",
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum Problem {
    NoKey { title: String },
    Missing { key: String, fields: Vec<&'static str> },
    DuplicateKey(String),
}

/// Problems that would make BibTeX warn: missing required fields and duplicate keys.
pub fn check(entries: &[&Entry]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for e in entries {
        if e.key.is_empty() {
            problems.push(Problem::NoKey { title: e.title.clone() });
            continue;
        }
        *seen.entry(e.key.as_str()).or_default() += 1;
        let venue_name = match e.kind {
            EntryType::Article => Some("journal"),
            EntryType::InProceedings | EntryType::InCollection => Some("booktitle"),
            _ => None,
        };
        let publisher_name = match e.kind {
            EntryType::Book | EntryType::InCollection => Some("publisher"),
            EntryType::PhdThesis => Some("school"),
            EntryType::TechReport => Some("institution"),
            _ => None,
        };
        let mut missing = Vec::new();
        if e.authors.is_empty() {
            missing.push("author");
        }
        if e.kind != EntryType::Misc && e.year.is_none() {
            missing.push("year");
        }
        if e.kind == EntryType::Article && e.volume.trim().is_empty() && e.number.trim().is_empty() {
            missing.push("volume/number");
        }
        if e.title.trim().is_empty() {
            missing.push("title");
        }
        if let Some(name) = venue_name.filter(|_| e.venue.trim().is_empty()) {
            missing.push(name);
        }
        if let Some(name) = publisher_name.filter(|_| e.publisher.trim().is_empty()) {
            missing.push(name);
        }
        if !missing.is_empty() {
            problems.push(Problem::Missing {
                key: e.key.clone(),
                fields: missing,
            });
        }
    }
    let mut dups: Vec<_> = seen.into_iter().filter(|(_, n)| *n > 1).map(|(k, _)| k).collect();
    dups.sort();
    problems.extend(dups.into_iter().map(|k| Problem::DuplicateKey(k.to_string())));
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(family: &str, given: &str) -> Person {
        Person {
            family: family.into(),
            given: given.into(),
            ..Default::default()
        }
    }

    #[test]
    fn key_format() {
        let e = Entry {
            authors: vec![person("Müller", "Jörg")],
            year: Some(2020),
            venue_short: "NeurIPS".into(),
            ..Default::default()
        };
        assert_eq!(cite_key(&e), "Muller2020NeurIPS");
        let e = Entry {
            authors: vec![person("van der Maaten", "Laurens")],
            year: Some(2008),
            ..Default::default()
        };
        assert_eq!(cite_key(&e), "VanDerMaaten2008");
        assert_eq!(sanitize_key("He 2016, CVPR{}"), "He2016CVPR");
    }

    #[test]
    fn escapes_text() {
        assert_eq!(latex_text("R&D 50% #1 a_b"), "R\\&D 50\\% \\#1 a\\_b");
        assert_eq!(latex_text("Gödel–Escher"), "G{\\\"o}del--Escher");
        assert_eq!(latex_text("Çelik Škoda"), "{\\c{C}}elik {\\v{S}}koda");
        assert_eq!(latex_text("α < β"), "$\\alpha$ $<$ $\\beta$");
    }

    #[test]
    fn protects_title_case() {
        assert_eq!(
            latex_title("BERT: Pre-training of Deep Bidirectional Transformers"),
            "{BERT}: Pre-training of Deep Bidirectional Transformers"
        );
        assert_eq!(latex_title("Faster R-CNN (with GPUs)"), "Faster {R-CNN} (with {GPUs})");
        assert_eq!(latex_title("ImageNet in 3D"), "{ImageNet} in {3D}");
        assert_eq!(
            latex_title("Towards Real-Time Detection"),
            "Towards Real-Time Detection"
        );
    }

    #[test]
    fn formats_entry() {
        let e = Entry {
            key: "He2016CVPR".into(),
            kind: EntryType::InProceedings,
            title: "Deep Residual Learning for Image Recognition".into(),
            authors: vec![person("He", "Kaiming"), person("Zhang", "Xiangyu")],
            year: Some(2016),
            month: Some(6),
            venue: "2016 IEEE Conference on Computer Vision and Pattern Recognition (CVPR)".into(),
            pages: "770-778".into(),
            publisher: "IEEE".into(),
            doi: "10.1109/CVPR.2016.90".into(),
            ..Default::default()
        };
        let expected = "\
@inproceedings{He2016CVPR,
  author    = {He, Kaiming and Zhang, Xiangyu},
  title     = {Deep Residual Learning for Image Recognition},
  booktitle = {2016 IEEE Conference on Computer Vision and Pattern Recognition (CVPR)},
  pages     = {770--778},
  year      = {2016},
  month     = jun,
  publisher = {IEEE},
  doi       = {10.1109/CVPR.2016.90}
}
";
        assert_eq!(entry_to_bibtex(&e), expected);
    }

    #[test]
    fn arxiv_entry() {
        let e = Entry {
            key: "Vaswani2017arXiv".into(),
            kind: EntryType::Misc,
            title: "Attention Is All You Need".into(),
            authors: vec![person("Vaswani", "Ashish")],
            year: Some(2017),
            eprint: "1706.03762".into(),
            primary_class: "cs.CL".into(),
            doi: "10.48550/arXiv.1706.03762".into(),
            url: "https://arxiv.org/abs/1706.03762".into(),
            ..Default::default()
        };
        let bib = entry_to_bibtex(&e);
        assert!(
            bib.contains("howpublished  = {arXiv preprint arXiv:1706.03762}"),
            "{bib}"
        );
        assert!(bib.contains("archiveprefix = {arXiv}"));
        assert!(bib.contains("url           = {https://arxiv.org/abs/1706.03762}"));
    }

    #[test]
    fn reports_problems() {
        let a = Entry {
            key: "k".into(),
            kind: EntryType::Article,
            title: "T".into(),
            ..Default::default()
        };
        let b = a.clone();
        let problems = check(&[&a, &b]);
        let missing = Problem::Missing {
            key: "k".into(),
            fields: vec!["author", "year", "volume/number", "journal"],
        };
        assert_eq!(problems, [missing.clone(), missing, Problem::DuplicateKey("k".into())]);
    }

    #[test]
    fn accent_table_is_consistent() {
        for (cmd, chars, bases) in ACCENTS {
            assert_eq!(chars.chars().count(), bases.chars().count(), "{cmd}");
        }
    }
}
