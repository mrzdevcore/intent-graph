//! `ig.yaml` → `Config`.

use serde_yaml_ng::{Mapping, Value};

/// An `ig.yaml` error; the message starts with `ig.yaml: `.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

pub type Result<T> = std::result::Result<T, ConfigError>;

macro_rules! bail {
    ($($t:tt)*) => {
        return Err(ConfigError(format!($($t)*)))
    };
}

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

/// `blueprint`: HTTP client calls linked to route nodes, and npm packages that stand for external
/// systems.
#[derive(Debug, Clone)]
pub struct Blueprint {
    pub http_calls: Vec<String>,
    pub route_kind: String,
    /// Path prefix the client adds in front of route paths (a proxy or server prefix), e.g. `/api`.
    pub http_prefix: String,
    pub systems: Vec<(String, Vec<String>)>,
}

impl Default for Blueprint {
    fn default() -> Self {
        Blueprint {
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
    pub embedded: Vec<String>,
    /// Generated declaration files (globs) listing a framework's auto-imports.
    pub auto_imports: Vec<String>,
    pub ignore: Vec<String>,
    pub tests: Vec<String>,
    pub test_calls: Vec<String>,
    pub patterns: Vec<Pattern>,
    pub blueprint: Blueprint,
    /// The AI agent `ig view` asks to read the architecture: claude, gemini, codex or ollama.
    pub agent: Option<String>,
    /// Removed keys found in the file; they are ignored.
    pub notices: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            layers: vec![],
            rules: vec![],
            embedded: vec![],
            auto_imports: vec![],
            ignore: vec![],
            tests: vec![
                "**/*.test.*".into(),
                "**/*.spec.*".into(),
                "**/__tests__/**".into(),
            ],
            test_calls: vec!["describe".into(), "it".into(), "test".into()],
            patterns: vec![],
            blueprint: Blueprint::default(),
            agent: None,
            notices: vec![],
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Config> {
        let doc: Value =
            serde_yaml_ng::from_str(text).map_err(|e| ConfigError(format!("ig.yaml: {e}")))?;
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
                "autoImports" => c.auto_imports = strings(v, "autoImports")?,
                "ignore" => c.ignore = strings(v, "ignore")?,
                "tests" => c.tests = strings(v, "tests")?,
                "testCalls" => c.test_calls = strings(v, "testCalls")?,
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
                "blueprint" => c.blueprint = blueprint(v, &mut c.notices)?,
                "agent" => match v.as_str() {
                    Some(a @ ("claude" | "gemini" | "codex" | "ollama")) => {
                        c.agent = Some(a.into())
                    }
                    _ => bail!("ig.yaml: agent: expected claude, gemini, codex or ollama"),
                },
                "complexity" | "complexityMax" | "checkers" | "severity" | "blockOn" | "editor"
                | "hookMaxRounds" | "entryPoints" | "review" => c.notices.push(key.to_string()),
                _ => bail!("ig.yaml: {}: unknown key", scalar(k)),
            }
        }
        Ok(c)
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
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}|{:?}",
            self.embedded,
            self.ignore,
            self.tests,
            self.test_calls,
            pats,
            b.http_calls,
            b.route_kind,
            b.http_prefix,
            b.systems,
            self.auto_imports
        )
    }
}

fn blueprint(v: &Value, notices: &mut Vec<String>) -> Result<Blueprint> {
    let mut b = Blueprint::default();
    for (k, x) in mapping(v, "blueprint")? {
        let k = scalar(k);
        match k.as_str() {
            "lanes" => notices.push("blueprint.lanes".into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_keys_are_ignored_with_notice() {
        let c = Config::parse("complexityMax: 10\ncheckers: []\nreview: {}\n").unwrap();
        assert!(c.notices.contains(&"complexityMax".to_string()));
    }

    #[test]
    fn parses_and_validates() {
        let c = Config::parse("layers:\n  ui: [apps/web]\n  core: ['packages/core/**']\nrules:\n  - forbid: \"core -> ui\"\n").unwrap();
        assert_eq!(c.layers.len(), 2);
        assert_eq!(c.rules, vec![("core".into(), "ui".into())]);
        let e = Config::parse("rules:\n  - forbid: nope\n")
            .unwrap_err()
            .to_string();
        assert_eq!(e, "ig.yaml: rules[0]: expected \"a -> b\"");
        let b = Config::parse(
            "blueprint:\n  lanes:\n    - { name: User, match: ['src/pages/**'] }\n  http: { calls: [fetch] }\n  systems:\n    PostgreSQL: [pg, knex]\n",
        )
        .unwrap()
        .blueprint;
        assert_eq!(b.route_kind, "route");
        assert_eq!(b.systems[0].1, ["pg", "knex"]);
        assert_eq!(
            Config::parse("agent: codex\n").unwrap().agent.as_deref(),
            Some("codex")
        );
        let e = Config::parse("agent: gpt\n").unwrap_err().to_string();
        assert_eq!(
            e,
            "ig.yaml: agent: expected claude, gemini, codex or ollama"
        );
        let e = Config::parse("ignore: x\n").unwrap_err().to_string();
        assert_eq!(e, "ig.yaml: ignore: expected a list");
    }
}
