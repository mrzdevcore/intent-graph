//! Single-file components: script extraction and markup scanning (regex-based).

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static SCRIPT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<script\b([^>]*)>(.*?)</script\s*>").unwrap());
static LANG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?i)\blang\s*=\s*["']?(tsx?)["']?"#).unwrap());
static MASK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>|<style\b[^>]*>.*?</style\s*>|<!--.*?-->").unwrap()
});
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<([A-Za-z][\w.-]*)[\s/>]").unwrap());
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z_$][\w$]*").unwrap());

pub struct Script {
    pub source: String,
    pub ts: bool,
    pub tsx: bool,
}

fn blank(bytes: &mut [u8]) {
    for b in bytes {
        if *b != b'\n' && *b != b'\r' {
            *b = b' ';
        }
    }
}

/// Keeps only `<script>` bodies, blanking everything else so offsets and lines are preserved.
pub fn extract_script(src: &str) -> Script {
    let mut out = src.as_bytes().to_vec();
    let mut keep = vec![false; out.len()];
    let (mut ts, mut tsx) = (false, false);
    for c in SCRIPT.captures_iter(src) {
        if let Some(l) = LANG.captures(&c[1]) {
            ts = true;
            tsx |= l[1].eq_ignore_ascii_case("tsx");
        }
        let body = c.get(2).unwrap();
        keep[body.start()..body.end()].iter_mut().for_each(|k| *k = true);
    }
    for (b, k) in out.iter_mut().zip(&keep) {
        if !*k && *b != b'\n' && *b != b'\r' {
            *b = b' ';
        }
    }
    let mut source = String::from_utf8(out).unwrap_or_default();
    source.push_str("\nexport {};");
    Script { source, ts, tsx }
}

#[derive(Debug, PartialEq)]
pub enum MarkupRef {
    /// A component tag (PascalCase or containing `-`).
    Tag(String),
    /// An identifier word, first use only.
    Word(String),
}

/// Scans markup outside script, style and comments. Returns (reference, byte offset).
pub fn scan_markup(src: &str) -> Vec<(MarkupRef, usize)> {
    let mut bytes = src.as_bytes().to_vec();
    for m in MASK.find_iter(src) {
        blank(&mut bytes[m.start()..m.end()]);
    }
    let masked = String::from_utf8(bytes).unwrap_or_default();
    let mut out = vec![];
    for c in TAG.captures_iter(&masked) {
        let name = c.get(1).unwrap();
        let s = name.as_str();
        if s.starts_with(|ch: char| ch.is_ascii_uppercase()) || s.contains('-') {
            out.push((MarkupRef::Tag(s.to_string()), name.start()));
        }
    }
    let mut seen = HashSet::new();
    for m in WORD.find_iter(&masked) {
        // Skip word fragments glued to a preceding identifier char (e.g. digits).
        let prev = masked[..m.start()].chars().next_back();
        if prev.is_some_and(|p| p.is_alphanumeric() || p == '_' || p == '$') {
            continue;
        }
        if seen.insert(m.as_str()) {
            out.push((MarkupRef::Word(m.as_str().to_string()), m.start()));
        }
    }
    out
}

/// `my-button` → `MyButton`.
pub fn pascal(tag: &str) -> String {
    tag.split('-')
        .map(|p| {
            let mut c = p.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_and_scans() {
        let src = "<template>\n  <MyBtn @click=\"go\"/>\n  <x-card/>\n</template>\n<script lang=\"ts\">\nimport MyBtn from './B.vue'\n</script>\n";
        let s = extract_script(src);
        assert!(s.ts);
        assert_eq!(s.source.len(), src.len() + "\nexport {};".len());
        assert_eq!(s.source.lines().count(), src.lines().count() + 1);
        assert!(s.source.contains("import MyBtn from './B.vue'"));
        assert!(!s.source.contains("template"));
        let refs = scan_markup(src);
        assert!(refs.contains(&(MarkupRef::Tag("MyBtn".into()), 14)));
        assert!(refs.iter().any(|(r, _)| *r == MarkupRef::Tag("x-card".into())));
        assert!(refs.iter().any(|(r, _)| *r == MarkupRef::Word("go".into())));
        assert!(!refs.iter().any(|(r, _)| *r == MarkupRef::Word("import".into())));
        assert_eq!(pascal("x-card"), "XCard");
    }
}
