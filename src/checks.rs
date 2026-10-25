//! Rules: arch, complexity, intents, SARIF checkers, baseline.

use crate::config::{Config, Format, Severity};
use crate::diff::{GraphDiff, Status};
use crate::extract::innermost_at;
use crate::intents::Intent;
use crate::model::{Finding, Graph, Node};
use crate::workspace::{self, Package};
use globset::GlobMatcher;
use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// Layer assignment: the most specific entry wins (longest literal prefix), ties go to the first declared.
pub struct Layers<'c> {
    entries: Vec<(&'c str, GlobMatcher, usize)>,
}

fn literal_prefix(g: &str) -> usize {
    g.find(['*', '?', '[', '{']).unwrap_or(g.len())
}

impl<'c> Layers<'c> {
    pub fn new(config: &'c Config, packages: &[Package]) -> Layers<'c> {
        let mut entries = vec![];
        for (name, globs) in &config.layers {
            for g in globs {
                let pattern = match packages.iter().find(|p| &p.name == g) {
                    Some(p) if p.dir == "." => "**".to_string(),
                    Some(p) => format!("{}/**", p.dir),
                    None => g.trim_start_matches("./").to_string(),
                };
                if let Ok(m) = workspace::glob(&pattern) {
                    entries.push((name.as_str(), m.compile_matcher(), literal_prefix(&pattern)));
                }
            }
        }
        Layers { entries }
    }

    pub fn of(&self, file: &str) -> Option<&'c str> {
        let mut best: Option<(&str, usize)> = None;
        for (name, m, spec) in &self.entries {
            if m.is_match(file) && best.is_none_or(|(_, s)| *spec > s) {
                best = Some((name, *spec));
            }
        }
        best.map(|(n, _)| n)
    }
}

pub fn arch(graph: &Graph, config: &Config, packages: &[Package]) -> Vec<Finding> {
    if config.rules.is_empty() {
        return vec![];
    }
    let layers = Layers::new(config, packages);
    let files: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.file.as_str()))
        .collect();
    let layer_of: HashMap<&str, Option<&str>> =
        files.values().map(|f| (*f, layers.of(f))).collect();
    let layer = |f: &str| layer_of.get(f).copied().flatten();
    let mut out = vec![];
    for e in graph
        .edges
        .iter()
        .filter(|e| e.resolved() && matches!(e.kind.as_str(), "imports" | "calls" | "renders"))
    {
        let (Some(ff), Some(tf)) = (files.get(e.from.as_str()), files.get(e.to.as_str())) else {
            continue;
        };
        let (Some(a), Some(b)) = (layer(ff), layer(tf)) else {
            continue;
        };
        if a == b || !config.rules.iter().any(|(x, y)| x == a && y == b) {
            continue;
        }
        out.push(
            Finding::new(
                "ig/arch",
                Severity::Error,
                "ig",
                &e.file,
                e.line,
                format!(
                    "layer {a} must not depend on {b}: {} {} {}",
                    e.from, e.kind, e.to
                ),
            )
            .node(&e.from)
            .edge(&e.from, &e.to)
            .fix(format!(
                "Remove this dependency, or move the code so {a} no longer {} {b}.",
                e.kind
            )),
        );
    }
    out
}

pub fn complexity(
    head: &Graph,
    base: Option<&Graph>,
    diff: &GraphDiff,
    config: &Config,
) -> Vec<Finding> {
    let before: HashMap<&str, &Node> = base
        .map(|b| b.nodes.iter().map(|n| (n.id.as_str(), n)).collect())
        .unwrap_or_default();
    let (max, inc) = (config.complexity_max, config.complexity_max_increase);
    let mut out = vec![];
    for n in head.nodes.iter().filter(|n| !n.is_module()) {
        let c = n.metrics.complexity;
        let msg = match (diff.status(&n.id), before.get(n.id.as_str())) {
            (Status::Added, _) if c > max => format!("{} has complexity {c} (max {max})", n.name),
            (Status::Changed, Some(b)) if c > max && c > b.metrics.complexity => {
                format!(
                    "{} has complexity {c} (max {max}, was {})",
                    n.name, b.metrics.complexity
                )
            }
            (Status::Changed, Some(b)) if c > b.metrics.complexity + inc => {
                format!(
                    "{} complexity rose from {} to {c} (max increase {inc})",
                    n.name, b.metrics.complexity
                )
            }
            (Status::Unchanged, _) if c > max => {
                format!("{} has complexity {c} (max {max})", n.name)
            }
            _ => continue,
        };
        out.push(
            Finding::new(
                "ig/complexity",
                Severity::Error,
                "ig",
                &n.file,
                n.range.start_line,
                msg,
            )
            .node(&n.id)
            .fix("Split it into smaller functions or flatten the branching."),
        );
    }
    out
}

/// `scope`: the intents in scope (changed in intents.md, or all with `--all`).
pub fn intents(
    head: &Graph,
    declared: &[Intent],
    scope: &BTreeSet<String>,
    tests: &globset::GlobSet,
) -> Vec<Finding> {
    let mut implemented: HashSet<&str> = HashSet::new();
    let mut tested: HashSet<&str> = HashSet::new();
    let ids: HashSet<&str> = declared.iter().map(|i| i.id.as_str()).collect();
    let mut out = vec![];
    for n in &head.nodes {
        let in_test = n.kind == "test" || tests.is_match(&n.file);
        for t in &n.intents {
            if in_test {
                tested.insert(t);
            } else {
                implemented.insert(t);
            }
            if !ids.contains(t.as_str()) {
                out.push(
                    Finding::new(
                        "ig/intent-unknown",
                        Severity::Error,
                        "ig",
                        &n.file,
                        n.range.start_line,
                        format!("@intent {t} is not declared in intents.md"),
                    )
                    .node(&n.id)
                    .fix(format!(
                        "Add `- {t}: <one line>` to intents.md, or fix the tag."
                    )),
                );
            }
        }
    }
    for i in declared.iter().filter(|i| scope.contains(&i.id)) {
        if !implemented.contains(i.id.as_str()) {
            out.push(
                Finding::new(
                    "ig/intent-unimplemented",
                    Severity::Error,
                    "ig",
                    "intents.md",
                    i.line,
                    format!("intent {} has no implementation: {}", i.id, i.text),
                )
                .fix(format!(
                    "Tag the implementing code with `// @intent {}`.",
                    i.id
                )),
            );
        }
        if !tested.contains(i.id.as_str()) {
            out.push(
                Finding::new(
                    "ig/intent-untested",
                    Severity::Warning,
                    "ig",
                    "intents.md",
                    i.line,
                    format!("intent {} has no test: {}", i.id, i.text),
                )
                .fix(format!("Tag a test with `// @intent {}`.", i.id)),
            );
        }
    }
    out
}

pub struct CheckerOutput {
    pub findings: Vec<Finding>,
    pub errors: Vec<Finding>,
}

const TIMEOUT: Duration = Duration::from_secs(120);
const CAP: usize = 64 << 20;

fn read_capped(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || {
        let mut buf = vec![];
        let mut chunk = [0u8; 65536];
        let mut over = false;
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) if buf.len() + n > CAP => {
                    over = true;
                    buf.extend_from_slice(&chunk[..CAP - buf.len()]);
                }
                Ok(n) if !over => buf.extend_from_slice(&chunk[..n]),
                Ok(_) => {}
            }
        }
        (buf, over)
    })
}

/// Runs one checker as `sh -c <run>` in `root`.
fn run_checker(root: &Path, run: &str) -> Result<(Vec<u8>, Option<i32>, String), String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(run)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let out = read_capped(child.stdout.take().unwrap());
    let err = read_capped(child.stderr.take().unwrap());
    let start = Instant::now();
    let mut code = None;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                code = st.code();
                break false;
            }
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break true;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let (stdout, over) = out.join().unwrap_or_default();
    let (stderr, _) = err.join().unwrap_or_default();
    let stderr: String = String::from_utf8_lossy(&stderr).chars().take(300).collect();
    if timed_out {
        return Err(format!("timed out after 120 s; stderr: {stderr}"));
    }
    if over {
        return Err(format!("output exceeded 64 MB; stderr: {stderr}"));
    }
    Ok((stdout, code, stderr))
}

fn repo_path(root: &Path, uri: &str) -> String {
    let mut p = uri.strip_prefix("file://").unwrap_or(uri).to_string();
    if let Ok(d) = urlish_decode(&p) {
        p = d;
    }
    let path = Path::new(&p);
    if path.is_absolute() {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(r) = workspace::rel(root, &canon).or_else(|| workspace::rel(root, path)) {
            return r;
        }
    }
    workspace::normalize(p.trim_start_matches("./"))
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

/// Attaches the innermost graph node to each finding (module-level files only).
pub(crate) fn attach_nodes(head: &Graph, findings: Vec<Finding>) -> Vec<Finding> {
    let mut by_file: HashMap<&str, Vec<Node>> = HashMap::new();
    for n in &head.nodes {
        by_file.entry(n.file.as_str()).or_default().push(n.clone());
    }
    for v in by_file.values_mut() {
        v.sort_by_key(|n| !n.is_module());
    }
    findings
        .into_iter()
        .map(|f| match by_file.get(f.file.as_str()) {
            Some(nodes) if nodes.first().is_some_and(Node::is_module) => {
                let id = innermost_at(nodes, f.line).to_string();
                f.node(&id)
            }
            _ => f,
        })
        .collect()
}

pub fn parse_output(
    root: &Path,
    tool: &str,
    format: Format,
    bytes: &[u8],
    head: &Graph,
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
    Ok(attach_nodes(head, found))
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

pub fn parse_sarif(root: &Path, tool: &str, bytes: &[u8]) -> Result<Vec<Finding>, String> {
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

/// Runs all checkers (in parallel) against the tree at `root`.
pub fn checkers(root: &Path, config: &Config, head: &Graph) -> CheckerOutput {
    use rayon::prelude::*;
    let results: Vec<(String, Result<Vec<Finding>, String>)> = config
        .checkers
        .par_iter()
        .map(|c| {
            let r = run_checker(root, &c.run).and_then(|(b, code, stderr)| {
                let tail = |t: &str| t.chars().take(300).collect::<String>();
                let found = parse_output(root, &c.name, c.format, &b, head)
                    .map_err(|e| format!("{e}; stderr: {stderr}"))?;
                match code {
                    Some(0) => Ok(found),
                    _ if !found.is_empty() => Ok(found),
                    _ => {
                        let msg = if stderr.trim().is_empty() {
                            tail(&String::from_utf8_lossy(&b))
                        } else {
                            stderr
                        };
                        let code = code.map_or("a signal".into(), |c| c.to_string());
                        Err(format!("exited {code} with no findings; stderr: {msg}"))
                    }
                }
            });
            (c.name.clone(), r)
        })
        .collect();
    let mut out = CheckerOutput {
        findings: vec![],
        errors: vec![],
    };
    for (name, r) in results {
        match r {
            Ok(f) => out.findings.extend(f),
            Err(e) => out.errors.push(Finding::new(
                "ig/checker-error",
                Severity::Warning,
                &name,
                "ig.yaml",
                1,
                format!("checker {name}: {e}"),
            )),
        }
    }
    out
}

/// Baseline entries: fingerprint strings, or `{"fingerprint", "reason"}` exceptions. Empty when
/// the file is absent; an error when it exists but doesn't parse.
pub(crate) fn read_baseline(root: &Path) -> anyhow::Result<Vec<serde_json::Value>> {
    let path = root.join(".ig/baseline.json");
    match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| anyhow::anyhow!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(anyhow::anyhow!("{}: {e}", path.display())),
    }
}

fn baseline_items(root: &Path) -> Vec<serde_json::Value> {
    read_baseline(root).unwrap_or_default()
}

pub fn load_baseline(root: &Path) -> HashSet<String> {
    baseline_items(root)
        .iter()
        .filter_map(|v| v.as_str().or_else(|| v["fingerprint"].as_str()))
        .map(String::from)
        .collect()
}

/// Writes `fingerprints` as strings, keeping existing exception objects.
pub fn write_baseline(
    root: &Path,
    fingerprints: impl IntoIterator<Item = String>,
) -> anyhow::Result<()> {
    let set: BTreeSet<String> = fingerprints.into_iter().collect();
    let mut items: Vec<serde_json::Value> = set.into_iter().map(Into::into).collect();
    items.extend(baseline_items(root).into_iter().filter(|v| v.is_object()));
    std::fs::create_dir_all(root.join(".ig"))?;
    std::fs::write(
        root.join(".ig/baseline.json"),
        serde_json::to_string_pretty(&items)? + "\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_most_specific_wins() {
        let c = Config::parse(
            "layers:\n  a: ['packages/**']\n  b: ['packages/core/**']\n  c: ['packages/core/**']\n",
        )
        .unwrap();
        let l = Layers::new(&c, &[]);
        assert_eq!(l.of("packages/core/x.ts"), Some("b"));
        assert_eq!(l.of("packages/ui/x.ts"), Some("a"));
        assert_eq!(l.of("apps/x.ts"), None);
    }

    #[test]
    fn checker_formats() {
        let root = Path::new("/repo");
        let g = Graph::empty();
        let eslint = br#"[{"filePath":"/repo/src/a.ts","messages":[{"ruleId":"no-var","severity":2,"message":"Unexpected var","line":3}]}]"#;
        let f = parse_output(root, "eslint", Format::EslintJson, eslint, &g).unwrap();
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
        let f = parse_output(root, "stylelint", Format::StylelintJson, stylelint, &g).unwrap();
        assert_eq!(
            (f[0].rule_id.as_str(), f[0].line, f[0].severity),
            ("media-feature-range-notation", 7, Severity::Warning)
        );

        let cs = br#"<?xml version="1.0"?><checkstyle><file name="src/b.ts"><error line="2" column="1" severity="error" message="Use an arrow function &amp; const" source="prefer-arrow"/></file></checkstyle>"#;
        let f = parse_output(root, "oxlint", Format::Checkstyle, cs, &g).unwrap();
        assert_eq!(
            (
                f[0].file.as_str(),
                f[0].message.as_str(),
                f[0].rule_id.as_str()
            ),
            ("src/b.ts", "Use an arrow function & const", "prefer-arrow")
        );

        let tsc = b"src/c.ts(4,10): error TS2322: Type 'string' is not assignable to type 'number'.\nnoise line\nsrc/d.ts:9:2: something odd\n";
        let f = parse_output(root, "tsc", Format::Lines, tsc, &g).unwrap();
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
            assert!(
                parse_output(root, "x", fmt, b"  \n", &g)
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn lines_checker_ignores_exit_status() {
        let dir = std::env::temp_dir().join(format!("ig-lines-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let c = Config::parse(
            "checkers:\n  - name: t\n    format: lines\n    run: \"echo 'src/a.ts:1: boom'; exit 3\"\n",
        )
        .unwrap();
        let out = checkers(&dir, &c, &Graph::empty());
        std::fs::remove_dir_all(&dir).ok();
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert_eq!(out.findings.len(), 1);
    }

    #[test]
    fn broken_checker_is_reported() {
        let dir = std::env::temp_dir().join(format!("ig-broken-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let run = |cmd: &str| {
            let c = Config::parse(&format!(
                "checkers:\n  - name: t\n    format: lines\n    run: \"{cmd}\"\n"
            ))
            .unwrap();
            checkers(&dir, &c, &Graph::empty())
        };
        let bad = run("echo boom >&2; exit 127");
        let ok = run("exit 0");
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(bad.errors.len(), 1);
        assert!(
            bad.errors[0].message.contains("boom"),
            "{}",
            bad.errors[0].message
        );
        assert!(bad.errors[0].message.contains("exited 127"));
        assert!(ok.errors.is_empty() && ok.findings.is_empty());
    }

    #[test]
    fn sarif_paths() {
        let root = Path::new("/repo");
        assert_eq!(repo_path(root, "file:///repo/src/a%20b.ts"), "src/a b.ts");
        assert_eq!(repo_path(root, "./src/a.ts"), "src/a.ts");
    }
}
