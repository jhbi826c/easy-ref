//! Extracts DOIs from pasted text: bare DOIs, doi.org / publisher URLs, arXiv ids and URLs.

pub const ARXIV_PREFIX: &str = "10.48550/arxiv.";

#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    Doi(String),
    Unrecognized(String),
}

pub fn parse_input(input: &str) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    for raw in input.split_whitespace() {
        let token = match parse_token(raw) {
            Some(doi) => Token::Doi(doi),
            None => Token::Unrecognized(raw.to_string()),
        };
        if !out.contains(&token) {
            out.push(token);
        }
    }
    out
}

pub fn parse_token(raw: &str) -> Option<String> {
    let decoded = percent_decode(raw.trim());
    let is_url = decoded.contains("://");
    let mut s = decoded.as_str();
    if is_url {
        s = s.split(['?', '#']).next().unwrap_or(s);
    }
    let lower = s.to_ascii_lowercase();

    if let Some(pos) = find_doi_start(s) {
        let doi = trim_doi_tail(&s[pos..]);
        if doi.to_ascii_lowercase().starts_with(ARXIV_PREFIX) {
            let id = &doi[ARXIV_PREFIX.len()..];
            return Some(arxiv_doi(strip_arxiv_version(id)));
        }
        return Some(doi.to_string());
    }

    if let Some(idx) = lower.find("arxiv") {
        let mut rest = &s[idx + 5..];
        if let Some(p) = rest.find(".org/") {
            rest = &rest[p + 5..];
        }
        rest = rest.trim_start_matches([':', '/']);
        for prefix in ["abs/", "pdf/", "html/"] {
            if let Some(r) = rest.strip_prefix(prefix) {
                rest = r;
                break;
            }
        }
        let id = strip_arxiv_version(rest.trim_end_matches('/').trim_end_matches(".pdf"));
        return is_arxiv_id(id).then(|| arxiv_doi(id));
    }

    let id = strip_arxiv_version(s);
    if is_arxiv_id(id) {
        return Some(arxiv_doi(id));
    }
    None
}

fn arxiv_doi(id: &str) -> String {
    format!("10.48550/arXiv.{id}")
}

pub fn arxiv_id_from_doi(doi: &str) -> Option<&str> {
    let lower = doi.to_ascii_lowercase();
    lower.starts_with(ARXIV_PREFIX).then(|| &doi[ARXIV_PREFIX.len()..])
}

/// Position of `10.NNNN/` in `s`.
fn find_doi_start(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while let Some(off) = s[i..].find("10.") {
        let start = i + off;
        let mut j = start + 3;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        let digits = j - (start + 3);
        let boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        if boundary && (4..=9).contains(&digits) && bytes.get(j) == Some(&b'/') && j + 1 < bytes.len() {
            return Some(start);
        }
        i = start + 3;
    }
    None
}

fn trim_doi_tail(doi: &str) -> &str {
    let mut d = doi;
    loop {
        let before = d;
        d = d.trim_end_matches(['.', ',', ';', ':', '"', '\'', ']', '}', '>']);
        if d.ends_with(')') && d.matches('(').count() < d.matches(')').count() {
            d = &d[..d.len() - 1];
        }
        for suffix in ["/abstract", "/full", "/pdf", "/epdf"] {
            if let Some(stripped) = d.strip_suffix(suffix) {
                d = stripped;
            }
        }
        if d == before {
            return d;
        }
    }
}

fn strip_arxiv_version(id: &str) -> &str {
    match id.rfind('v') {
        Some(pos) if pos > 0 && pos + 1 < id.len() && id[pos + 1..].bytes().all(|b| b.is_ascii_digit()) => &id[..pos],
        _ => id,
    }
}

/// New style `2301.01234`, old style `hep-th/9901001` or `math.GT/0309136`.
fn is_arxiv_id(id: &str) -> bool {
    if let Some((yymm, num)) = id.split_once('.') {
        if yymm.len() == 4
            && yymm.bytes().all(|b| b.is_ascii_digit())
            && (4..=5).contains(&num.len())
            && num.bytes().all(|b| b.is_ascii_digit())
        {
            return true;
        }
    }
    if let Some((archive, num)) = id.split_once('/') {
        let archive_ok =
            !archive.is_empty() && archive.chars().all(|c| c.is_ascii_alphabetic() || c == '-' || c == '.');
        return archive_ok && num.len() == 7 && num.bytes().all(|b| b.is_ascii_digit());
    }
    false
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doi(s: &str) -> Option<String> {
        parse_token(s)
    }

    #[test]
    fn recognizes_doi_forms() {
        let want = Some("10.1145/3290605.3300857".to_string());
        assert_eq!(doi("10.1145/3290605.3300857"), want);
        assert_eq!(doi("https://doi.org/10.1145/3290605.3300857"), want);
        assert_eq!(doi("https://dl.acm.org/doi/abs/10.1145/3290605.3300857"), want);
        assert_eq!(doi("https://dl.acm.org/doi/10.1145/3290605.3300857?casa_token=x"), want);
        assert_eq!(doi("doi:10.1145/3290605.3300857."), want);
        assert_eq!(doi("https://doi.org/10.1145%2F3290605.3300857"), want);
        assert_eq!(doi("(10.1109/CVPR.2016.90)"), Some("10.1109/CVPR.2016.90".into()));
        assert_eq!(
            doi("https://link.springer.com/chapter/10.1007/978-3-319-46448-0_2"),
            Some("10.1007/978-3-319-46448-0_2".into())
        );
        assert_eq!(
            doi("https://onlinelibrary.wiley.com/doi/full/10.1002/anie.201915678"),
            Some("10.1002/anie.201915678".into())
        );
    }

    #[test]
    fn recognizes_arxiv_forms() {
        let want = Some("10.48550/arXiv.1706.03762".to_string());
        assert_eq!(doi("1706.03762"), want);
        assert_eq!(doi("1706.03762v5"), want);
        assert_eq!(doi("arXiv:1706.03762"), want);
        assert_eq!(doi("https://arxiv.org/abs/1706.03762v7"), want);
        assert_eq!(doi("https://arxiv.org/pdf/1706.03762.pdf"), want);
        assert_eq!(doi("https://doi.org/10.48550/arXiv.1706.03762"), want);
        assert_eq!(doi("10.48550/ARXIV.1706.03762"), want);
        assert_eq!(
            doi("https://arxiv.org/abs/hep-th/9901001"),
            Some("10.48550/arXiv.hep-th/9901001".into())
        );
    }

    #[test]
    fn rejects_non_identifiers() {
        assert_eq!(doi("https://ieeexplore.ieee.org/document/7780459"), None);
        assert_eq!(doi("hello"), None);
        assert_eq!(doi("2016.90"), None);
    }

    #[test]
    fn splits_and_dedupes_input() {
        let tokens = parse_input("10.1109/CVPR.2016.90\n 1706.03762 foo 10.1109/CVPR.2016.90");
        assert_eq!(
            tokens,
            [
                Token::Doi("10.1109/CVPR.2016.90".into()),
                Token::Doi("10.48550/arXiv.1706.03762".into()),
                Token::Unrecognized("foo".into()),
            ]
        );
    }
}
