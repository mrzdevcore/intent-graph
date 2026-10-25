//! `ig.yaml` → `Config`.

use anyhow::{Result, bail};
use serde_yaml_ng::{Mapping, Value};
use std::path::Path;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Note,
    Warning,
    Error,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> {
        match s {
            "error" => Some(Severity::Error),
            "warning" => Some(Severity::Warning),
            "note" => Some(Severity::Note),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub kind: String,
    pub matcher: String,
    pub where_: Vec<(String, Vec<String>)>,
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct Checker {
    pub name: String,
    pub run: String,
    pub format: Format,
    /// Globs naming the tool's own config files (tamper detection).
    pub configs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Sarif,
    Checkstyle,
    EslintJson,
    StylelintJson,
    Lines,
}

impl Format {
    fn parse(s: &str) -> Option<Format> {
        Some(match s {
            "sarif" => Format::Sarif,
            "checkstyle" => Format::Checkstyle,
            "eslint-json" => Format::EslintJson,
            "stylelint-json" => Format::StylelintJson,
            "lines" => Format::Lines,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ReviewConfig {
    pub hub_percentile: f64,
    pub hub_min: usize,
    pub instability_gap: f64,
    pub stable_fan_in: usize,
    pub cohesion_groups: usize,
    pub cohesion_min_members: usize,
    pub coupling_ratio: f64,
    pub coupling_min: usize,
    pub history_commits: usize,
    pub oversized_loc: usize,
    pub oversized_complexity: usize,
    pub top: usize,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        ReviewConfig {
            hub_percentile: 0.95,
            hub_min: 5,
            instability_gap: 0.3,
            stable_fan_in: 5,
            cohesion_groups: 3,
            cohesion_min_members: 8,
            coupling_ratio: 0.6,
            coupling_min: 5,
            history_commits: 500,
            oversized_loc: 500,
            oversized_complexity: 25,
            top: 10,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Lane {
    pub name: String,
    pub matches: Vec<String>,
    pub exclude: Vec<String>,
    pub layers: Vec<String>,
}

/// `blueprint`: swimlanes, HTTP client calls linked to route nodes, and npm packages that stand for
/// external systems.
#[derive(Debug, Clone)]
pub struct Blueprint {
    pub lanes: Vec<Lane>,
    pub http_calls: Vec<String>,
    pub route_kind: String,
    /// Path prefix the client adds in front of route paths (a proxy or server prefix), e.g. `/api`.
    pub http_prefix: String,
    pub systems: Vec<(String, Vec<String>)>,
}

impl Default for Blueprint {
    fn default() -> Self {
        Blueprint {
            lanes: vec![],
            http_calls: vec![],
            route_kind: "route".into(),
            http_prefix: String::new(),
            systems: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub layers: Vec<(String, Vec<String>)>,
    pub rules: Vec<(String, String)>,
    pub complexity_max: u32,
    pub complexity_max_increase: u32,
    pub embedded: Vec<String>,
    pub ignore: Vec<String>,
    pub tests: Vec<String>,
    pub test_calls: Vec<String>,
    pub patterns: Vec<Pattern>,
    pub checkers: Vec<Checker>,
    pub severity: Vec<(String, Severity)>,
    pub block_on: Severity,
    pub editor: String,
    pub hook_max_rounds: u32,
    pub entry_points: Vec<String>,
    pub review: ReviewConfig,
    pub blueprint: Blueprint,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            layers: vec![],
            rules: vec![],
            complexity_max: 15,
            complexity_max_increase: 5,
            embedded: vec![],
            ignore: vec![],
            tests: vec![
                "**/*.test.*".into(),
                "**/*.spec.*".into(),
                "**/__tests__/**".into(),
            ],
            test_calls: vec!["describe".into(), "it".into(), "test".into()],
            patterns: vec![],
            checkers: vec![],
            severity: vec![],
            block_on: Severity::Error,
            editor: "zed {file}:{line}".into(),
            hook_max_rounds: 3,
            entry_points: vec![],
            review: ReviewConfig::default(),
            blueprint: Blueprint::default(),
        }
    }
}

impl Config {
    pub fn load(root: &Path) -> Result<Config> {
        match std::fs::read_to_string(root.join("ig.yaml")) {
            Ok(text) => Config::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => bail!("ig.yaml: {e}"),
        }
    }

    pub fn parse(text: &str) -> Result<Config> {
        let doc: Value =
            serde_yaml_ng::from_str(text).map_err(|e| anyhow::anyhow!("ig.yaml: {e}"))?;
        let mut c = Config::default();
        let map = match doc {
            Value::Null => return Ok(c),
            Value::Mapping(m) => m,
            _ => bail!("ig.yaml: expected a mapping"),
        };
        for (k, v) in &map {
            let key = k.as_str().unwrap_or("");
            match key {
                "layers" => {
                    for (name, entries) in mapping(v, "layers")? {
                        let name = name
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| scalar(name));
                        let p = format!("layers.{name}");
                        c.layers.push((name, strings(entries, &p)?));
                    }
                }
                "rules" => {
                    for (i, r) in list(v, "rules")?.iter().enumerate() {
                        let p = format!("rules[{i}]");
                        let forbid = r.get("forbid").and_then(Value::as_str);
                        let pair = forbid.and_then(|f| f.split_once("->"));
                        match pair {
                            Some((a, b)) if !a.trim().is_empty() && !b.trim().is_empty() => {
                                c.rules.push((a.trim().into(), b.trim().into()))
                            }
                            _ => bail!("ig.yaml: {p}: expected \"a -> b\""),
                        }
                    }
                }
                "complexity" => {
                    let m = mapping(v, "complexity")?;
                    if let Some(x) = m.get("max") {
                        c.complexity_max = uint(x, "complexity.max")? as u32;
                    }
                    if let Some(x) = m.get("maxIncrease") {
                        c.complexity_max_increase = uint(x, "complexity.maxIncrease")? as u32;
                    }
                }
                "embedded" => {
                    c.embedded = strings(v, "embedded")?
                        .into_iter()
                        .map(|e| {
                            if e.starts_with('.') {
                                e
                            } else {
                                format!(".{e}")
                            }
                        })
                        .collect()
                }
                "ignore" => c.ignore = strings(v, "ignore")?,
                "tests" => c.tests = strings(v, "tests")?,
                "testCalls" => c.test_calls = strings(v, "testCalls")?,
                "entryPoints" => c.entry_points = strings(v, "entryPoints")?,
                "patterns" => {
                    for (i, p) in list(v, "patterns")?.iter().enumerate() {
                        let path = format!("patterns[{i}]");
                        let field = |f: &str| -> Result<String> {
                            match p.get(f).and_then(Value::as_str) {
                                Some(s) => Ok(s.to_string()),
                                None => bail!("ig.yaml: {path}.{f}: expected a string"),
                            }
                        };
                        let mut where_ = vec![];
                        if let Some(w) = p.get("where") {
                            for (var, allowed) in mapping(w, &format!("{path}.where"))? {
                                let var = scalar(var);
                                let allowed = strings(allowed, &format!("{path}.where.{var}"))?;
                                where_.push((var, allowed));
                            }
                        }
                        c.patterns.push(Pattern {
                            kind: field("kind")?,
                            matcher: field("match")?,
                            where_,
                            id: field("id")?,
                        });
                    }
                }
                "checkers" => {
                    for (i, ch) in list(v, "checkers")?.iter().enumerate() {
                        let path = format!("checkers[{i}]");
                        let get = |f: &str| -> Result<String> {
                            match ch.get(f).and_then(Value::as_str) {
                                Some(s) => Ok(s.to_string()),
                                None => bail!("ig.yaml: {path}.{f}: expected a string"),
                            }
                        };
                        let format = match ch.get("format") {
                            None => Format::Sarif,
                            Some(f) => match f.as_str().and_then(Format::parse) {
                                Some(f) => f,
                                None => bail!(
                                    "ig.yaml: {path}.format: expected one of sarif, checkstyle, eslint-json, stylelint-json, lines"
                                ),
                            },
                        };
                        let configs = match ch.get("configs") {
                            Some(v) => strings(v, &format!("{path}.configs"))?,
                            None => vec![],
                        };
                        c.checkers.push(Checker {
                            name: get("name")?,
                            run: get("run")?,
                            format,
                            configs,
                        });
                    }
                }
                "severity" => {
                    for (rule, sev) in mapping(v, "severity")? {
                        let rule = scalar(rule);
                        c.severity
                            .push((rule.clone(), severity(sev, &format!("severity.{rule}"))?));
                    }
                }
                "blockOn" => c.block_on = severity(v, "blockOn")?,
                "editor" => c.editor = string(v, "editor")?,
                "hookMaxRounds" => c.hook_max_rounds = uint(v, "hookMaxRounds")? as u32,
                "review" => {
                    let r = &mut c.review;
                    for (k, x) in mapping(v, "review")? {
                        let k = scalar(k);
                        let p = format!("review.{k}");
                        match k.as_str() {
                            "hubPercentile" => r.hub_percentile = float(x, &p)?,
                            "hubMin" => r.hub_min = uint(x, &p)?,
                            "instabilityGap" => r.instability_gap = float(x, &p)?,
                            "stableFanIn" => r.stable_fan_in = uint(x, &p)?,
                            "cohesionGroups" => r.cohesion_groups = uint(x, &p)?,
                            "cohesionMinMembers" => r.cohesion_min_members = uint(x, &p)?,
                            "couplingRatio" => r.coupling_ratio = float(x, &p)?,
                            "couplingMin" => r.coupling_min = uint(x, &p)?,
                            "historyCommits" => r.history_commits = uint(x, &p)?,
                            "oversizedLoc" => r.oversized_loc = uint(x, &p)?,
                            "oversizedComplexity" => r.oversized_complexity = uint(x, &p)?,
                            "top" => r.top = uint(x, &p)?,
                            _ => bail!("ig.yaml: {p}: unknown key"),
                        }
                    }
                }
                "blueprint" => c.blueprint = blueprint(v)?,
                _ => bail!("ig.yaml: {}: unknown key", scalar(k)),
            }
        }
        Ok(c)
    }

    pub fn severity_for(&self, rule: &str) -> Option<Severity> {
        self.severity
            .iter()
            .find(|(r, _)| r == rule)
            .map(|(_, s)| *s)
    }

    /// Hash input for the base-graph cache key: everything that changes graph output.
    pub fn graph_key_input(&self) -> String {
        let pats: Vec<String> = self
            .patterns
            .iter()
            .map(|p| {
                format!(
                    "{}\u{1}{}\u{1}{:?}\u{1}{}",
                    p.kind, p.matcher, p.where_, p.id
                )
            })
            .collect();
        let b = &self.blueprint;
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}",
            self.embedded,
            self.ignore,
            self.tests,
            self.test_calls,
            pats,
            b.http_calls,
            b.route_kind,
            b.http_prefix,
            b.systems
        )
    }
}

fn blueprint(v: &Value) -> Result<Blueprint> {
    let mut b = Blueprint::default();
    for (k, x) in mapping(v, "blueprint")? {
        let k = scalar(k);
        match k.as_str() {
            "lanes" => {
                for (i, l) in list(x, "blueprint.lanes")?.iter().enumerate() {
                    let p = format!("blueprint.lanes[{i}]");
                    let name = match l.get("name").and_then(Value::as_str) {
                        Some(n) => n.to_string(),
                        None => bail!("ig.yaml: {p}.name: expected a string"),
                    };
                    let matches = l
                        .get("match")
                        .map(|m| strings(m, &format!("{p}.match")))
                        .transpose()?
                        .unwrap_or_default();
                    let layers = l
                        .get("layers")
                        .map(|m| strings(m, &format!("{p}.layers")))
                        .transpose()?
                        .unwrap_or_default();
                    let exclude = l
                        .get("exclude")
                        .map(|m| strings(m, &format!("{p}.exclude")))
                        .transpose()?
                        .unwrap_or_default();
                    if matches.is_empty() && layers.is_empty() {
                        bail!("ig.yaml: {p}: expected match or layers");
                    }
                    b.lanes.push(Lane {
                        name,
                        matches,
                        exclude,
                        layers,
                    });
                }
            }
            "http" => {
                for (hk, hv) in mapping(x, "blueprint.http")? {
                    match scalar(hk).as_str() {
                        "calls" => b.http_calls = strings(hv, "blueprint.http.calls")?,
                        "routes" => b.route_kind = string(hv, "blueprint.http.routes")?,
                        "prefix" => b.http_prefix = string(hv, "blueprint.http.prefix")?,
                        o => bail!("ig.yaml: blueprint.http.{o}: unknown key"),
                    }
                }
            }
            "systems" => {
                for (name, pkgs) in mapping(x, "blueprint.systems")? {
                    let name = scalar(name);
                    let pkgs = strings(pkgs, &format!("blueprint.systems.{name}"))?;
                    b.systems.push((name, pkgs));
                }
            }
            _ => bail!("ig.yaml: blueprint.{k}: unknown key"),
        }
    }
    Ok(b)
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn mapping<'a>(v: &'a Value, path: &str) -> Result<&'a Mapping> {
    match v {
        Value::Mapping(m) => Ok(m),
        _ => bail!("ig.yaml: {path}: expected a mapping"),
    }
}

fn list<'a>(v: &'a Value, path: &str) -> Result<&'a Vec<Value>> {
    match v {
        Value::Sequence(s) => Ok(s),
        _ => bail!("ig.yaml: {path}: expected a list"),
    }
}

fn string(v: &Value, path: &str) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        _ => bail!("ig.yaml: {path}: expected a string"),
    }
}

fn strings(v: &Value, path: &str) -> Result<Vec<String>> {
    list(v, path)?
        .iter()
        .enumerate()
        .map(|(i, x)| match x {
            Value::String(_) | Value::Number(_) | Value::Bool(_) => Ok(scalar(x)),
            _ => bail!("ig.yaml: {path}[{i}]: expected a string"),
        })
        .collect()
}

fn uint(v: &Value, path: &str) -> Result<usize> {
    match v.as_u64() {
        Some(n) => Ok(n as usize),
        None => bail!("ig.yaml: {path}: expected a non-negative integer"),
    }
}

fn float(v: &Value, path: &str) -> Result<f64> {
    match v.as_f64() {
        Some(n) => Ok(n),
        None => bail!("ig.yaml: {path}: expected a number"),
    }
}

fn severity(v: &Value, path: &str) -> Result<Severity> {
    match v.as_str().and_then(Severity::parse) {
        Some(s) => Ok(s),
        None => bail!("ig.yaml: {path}: expected one of error, warning, note"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checker_format_and_configs() {
        let c = Config::parse("checkers:\n  - name: s\n    run: x\n    format: lines\n    configs: ['.stylelintrc*']\n  - name: d\n    run: y\n").unwrap();
        assert_eq!(c.checkers[0].format, Format::Lines);
        assert_eq!(c.checkers[0].configs, vec![".stylelintrc*"]);
        assert_eq!(c.checkers[1].format, Format::Sarif);
        let e = Config::parse("checkers:\n  - name: s\n    run: x\n    format: nope\n").unwrap_err().to_string();
        assert_eq!(e, "ig.yaml: checkers[0].format: expected one of sarif, checkstyle, eslint-json, stylelint-json, lines");
    }

    #[test]
    fn parses_and_validates() {
        let c = Config::parse("layers:\n  ui: [apps/web]\n  core: ['packages/core/**']\nrules:\n  - forbid: \"core -> ui\"\nblockOn: warning\n").unwrap();
        assert_eq!(c.layers.len(), 2);
        assert_eq!(c.rules, vec![("core".into(), "ui".into())]);
        assert_eq!(c.block_on, Severity::Warning);
        let e = Config::parse("rules:\n  - forbid: nope\n")
            .unwrap_err()
            .to_string();
        assert_eq!(e, "ig.yaml: rules[0]: expected \"a -> b\"");
        let b = Config::parse(
            "blueprint:\n  lanes:\n    - { name: User, match: ['src/pages/**'] }\n  http: { calls: [fetch] }\n  systems:\n    PostgreSQL: [pg, knex]\n",
        )
        .unwrap()
        .blueprint;
        assert_eq!(b.lanes[0].name, "User");
        assert_eq!(b.route_kind, "route");
        assert_eq!(b.systems[0].1, ["pg", "knex"]);
        let e = Config::parse("blueprint:\n  lanes:\n    - { name: X }\n")
            .unwrap_err()
            .to_string();
        assert_eq!(e, "ig.yaml: blueprint.lanes[0]: expected match or layers");
        let e = Config::parse("ignore: x\n").unwrap_err().to_string();
        assert_eq!(e, "ig.yaml: ignore: expected a list");
        let e = Config::parse("severity:\n  ig/arch: bad\n")
            .unwrap_err()
            .to_string();
        assert!(e.contains("expected one of"));
    }
}
