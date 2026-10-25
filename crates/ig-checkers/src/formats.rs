//! Checker output formats: SARIF, checkstyle, ESLint and Stylelint JSON, and lines.

use ig_core::config::{Format, Severity};
use ig_core::model::Finding;
use ig_core::workspace::{normalize, rel};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

fn repo_path(root: &Path, uri: &str) -> String {
    let mut p = uri.strip_prefix("file://").unwrap_or(uri).to_string();
    if let Ok(d) = urlish_decode(&p) {
        p = d;
    }
    let path = Path::new(&p);
    if path.is_absolute() {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(r) = rel(root, &canon).or_else(|| rel(root, path)) {
            return r;
        }
    }
    normalize(p.trim_start_matches("./"))
}

fn urlish_decode(s: &str) -> Result<String, ()> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = std::str::from_utf8(&b[i + 1..i + 3]).map_err(|_| ())?;
            out.push(u8::from_str_radix(h, 16).map_err(|_| ())?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

/// Findings in `bytes`, a `tool` output in `format`; paths relative to `root`. No node ids.
pub fn parse_output(
    root: &Path,
    tool: &str,
    format: Format,
    bytes: &[u8],
) -> Result<Vec<Finding>, String> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(vec![]);
    }
    let found = match format {
        Format::Sarif => parse_sarif(root, tool, bytes)?,
        Format::Checkstyle => parse_checkstyle(root, tool, bytes)?,
        Format::EslintJson => parse_eslint(root, tool, bytes)?,
        Format::StylelintJson => parse_stylelint(root, tool, bytes)?,
        Format::Lines => parse_lines(root, tool, bytes),
    };
    Ok(found)
}

fn json_array(bytes: &[u8]) -> Result<Vec<serde_json::Value>, String> {
    match serde_json::from_slice(bytes) {
        Ok(serde_json::Value::Array(a)) => Ok(a),
        Ok(_) => Err("expected a JSON array".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn parse_eslint(root: &Path, tool: &str, bytes: &[u8]) -> Result<Vec<Finding>, String> {
    let mut out = vec![];
    for file in json_array(bytes)? {
        let path = repo_path(root, file["filePath"].as_str().unwrap_or(""));
        for m in file["messages"].as_array().into_iter().flatten() {
            let sev = if m["severity"].as_u64() == Some(2) {
                Severity::Error
            } else {
                Severity::Warning
            };
            out.push(Finding::new(
                m["ruleId"].as_str().unwrap_or(tool),
                sev,
                tool,
                &path,
                m["line"].as_u64().unwrap_or(1) as u32,
                m["message"].as_str().unwrap_or("").to_string(),
            ));
        }
    }
    Ok(out)
}

fn parse_stylelint(root: &Path, tool: &str, bytes: &[u8]) -> Result<Vec<Finding>, String> {
    let mut out = vec![];
    for file in json_array(bytes)? {
        let path = repo_path(root, file["source"].as_str().unwrap_or(""));
        for w in file["warnings"].as_array().into_iter().flatten() {
            let sev = if w["severity"].as_str() == Some("error") {
                Severity::Error
            } else {
                Severity::Warning
            };
            out.push(Finding::new(
                w["rule"].as_str().unwrap_or(tool),
                sev,
                tool,
                &path,
                w["line"].as_u64().unwrap_or(1) as u32,
                w["text"].as_str().unwrap_or("").to_string(),
            ));
        }
    }
    Ok(out)
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn xml_attrs(s: &str) -> HashMap<&str, String> {
    static ATTR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(\w+)="([^"]*)""#).unwrap());
    ATTR.captures_iter(s)
        .map(|c| (c.get(1).unwrap().as_str(), xml_unescape(&c[2])))
        .collect()
}

fn parse_checkstyle(root: &Path, tool: &str, bytes: &[u8]) -> Result<Vec<Finding>, String> {
    static FILE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<file\b([^>]*?)(?:/>|>(.*?)</file>)").unwrap());
    static ERR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<error\b([^>]*?)/?>").unwrap());
    let text = String::from_utf8_lossy(bytes);
    if !text.contains("<checkstyle") {
        return Err("no <checkstyle> element".into());
    }
    let mut out = vec![];
    for f in FILE.captures_iter(&text) {
        let path = repo_path(root, xml_attrs(&f[1]).get("name").map_or("", |s| s));
        for e in ERR.captures_iter(f.get(2).map_or("", |m| m.as_str())) {
            let a = xml_attrs(&e[1]);
            let sev = match a.get("severity").map(String::as_str) {
                Some("error") => Severity::Error,
                Some("info") => Severity::Note,
                _ => Severity::Warning,
            };
            let rule = a
                .get("source")
                .filter(|s| !s.is_empty())
                .map_or(tool, |s| s);
            out.push(Finding::new(
                rule,
                sev,
                tool,
                &path,
                a.get("line").and_then(|l| l.parse().ok()).unwrap_or(1),
                a.get("message").cloned().unwrap_or_default(),
            ));
        }
    }
    Ok(out)
}

fn parse_lines(root: &Path, tool: &str, bytes: &[u8]) -> Vec<Finding> {
    // tsc: `path(line,col): error TS1234: message`; generic: `path:line[:col][: ]message`
    static TSC: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+?)\((\d+),\d+\):\s*(.*)$").unwrap());
    static GEN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+?):(\d+)(?::\d+)?:?\s*(.*)$").unwrap());
    static CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bTS\d+\b").unwrap());
    static SEV: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\W*(error|warning)\b").unwrap());
    let mut out = vec![];
    for l in String::from_utf8_lossy(bytes).lines() {
        let Some(c) = TSC.captures(l).or_else(|| GEN.captures(l)) else {
            continue;
        };
        let msg = c[3].trim();
        let sev = match SEV.captures(msg).map(|s| s[1].to_lowercase()) {
            Some(s) if s == "error" => Severity::Error,
            _ => Severity::Warning,
        };
        let rule = CODE.find(msg).map_or(tool, |m| m.as_str());
        out.push(Finding::new(
            rule,
            sev,
            tool,
            &repo_path(root, &c[1]),
            c[2].parse().unwrap_or(1),
            msg.to_string(),
        ));
    }
    out
}

fn parse_sarif(root: &Path, tool: &str, bytes: &[u8]) -> Result<Vec<Finding>, String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let runs = v
        .get("runs")
        .and_then(|r| r.as_array())
        .ok_or("no runs[]")?;
    let mut out = vec![];
    for run in runs {
        for r in run
            .get("results")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            let loc = r.pointer("/locations/0/physicalLocation");
            let uri = loc
                .and_then(|l| l.pointer("/artifactLocation/uri"))
                .and_then(|u| u.as_str())
                .unwrap_or("");
            let file = repo_path(root, uri);
            let line = loc
                .and_then(|l| l.pointer("/region/startLine"))
                .and_then(|x| x.as_u64())
                .unwrap_or(1) as u32;
            let sev = match r.get("level").and_then(|l| l.as_str()) {
                Some("error") => Severity::Error,
                Some("note") | Some("none") => Severity::Note,
                _ => Severity::Warning,
            };
            let rule = r.get("ruleId").and_then(|x| x.as_str()).unwrap_or(tool);
            let msg = r
                .pointer("/message/text")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let mut f = Finding::new(rule, sev, tool, &file, line, msg);
            if let Some(fix) = r
                .pointer("/fixes/0/description/text")
                .and_then(|x| x.as_str())
            {
                f = f.fix(fix);
            }
            out.push(f);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checker_formats() {
        let root = Path::new("/repo");
        let eslint = br#"[{"filePath":"/repo/src/a.ts","messages":[{"ruleId":"no-var","severity":2,"message":"Unexpected var","line":3}]}]"#;
        let f = parse_output(root, "eslint", Format::EslintJson, eslint).unwrap();
        assert_eq!(
            (
                f[0].rule_id.as_str(),
                f[0].file.as_str(),
                f[0].line,
                f[0].severity
            ),
            ("no-var", "src/a.ts", 3, Severity::Error)
        );

        let stylelint = br#"[{"source":"/repo/src/a.scss","warnings":[{"line":7,"column":1,"rule":"media-feature-range-notation","severity":"warning","text":"Expected context notation"}]}]"#;
        let f = parse_output(root, "stylelint", Format::StylelintJson, stylelint).unwrap();
        assert_eq!(
            (f[0].rule_id.as_str(), f[0].line, f[0].severity),
            ("media-feature-range-notation", 7, Severity::Warning)
        );

        let cs = br#"<?xml version="1.0"?><checkstyle><file name="src/b.ts"><error line="2" column="1" severity="error" message="Use an arrow function &amp; const" source="prefer-arrow"/></file></checkstyle>"#;
        let f = parse_output(root, "oxlint", Format::Checkstyle, cs).unwrap();
        assert_eq!(
            (
                f[0].file.as_str(),
                f[0].message.as_str(),
                f[0].rule_id.as_str()
            ),
            ("src/b.ts", "Use an arrow function & const", "prefer-arrow")
        );

        let tsc = b"src/c.ts(4,10): error TS2322: Type 'string' is not assignable to type 'number'.\nnoise line\nsrc/d.ts:9:2: something odd\n";
        let f = parse_output(root, "tsc", Format::Lines, tsc).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(
            (
                f[0].file.as_str(),
                f[0].line,
                f[0].rule_id.as_str(),
                f[0].severity
            ),
            ("src/c.ts", 4, "TS2322", Severity::Error)
        );
        assert_eq!(
            (f[1].file.as_str(), f[1].line, f[1].severity),
            ("src/d.ts", 9, Severity::Warning)
        );

        for fmt in [
            Format::Sarif,
            Format::Checkstyle,
            Format::EslintJson,
            Format::StylelintJson,
            Format::Lines,
        ] {
            assert!(parse_output(root, "x", fmt, b"  \n").unwrap().is_empty());
        }
    }

    #[test]
    fn sarif_paths() {
        let root = Path::new("/repo");
        assert_eq!(repo_path(root, "file:///repo/src/a%20b.ts"), "src/a b.ts");
        assert_eq!(repo_path(root, "./src/a.ts"), "src/a.ts");
    }
}
