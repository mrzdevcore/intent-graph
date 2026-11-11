mod extract;

use extract::{Callee, Extracted, Local};
use ig_core::config::Config;
use ig_core::model::Edge;
use ig_core::ports::{FileFacts, Files, FrontendOutput, LanguageFrontend, PortError};
use ig_core::workspace::{Package, SourceFile, WorkspaceInfo, globset, join, normalize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

pub struct GoFrontend;

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(".", |(d, _)| d)
}

fn under(dir: &str, root: &str) -> Option<String> {
    match (root, dir) {
        (".", ".") => Some(String::new()),
        (".", d) => Some(d.into()),
        (r, d) if r == d => Some(String::new()),
        (r, d) => d.strip_prefix(&format!("{r}/")).map(String::from),
    }
}

fn module_path(go_mod: &str) -> Option<String> {
    go_mod.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("module")?;
        let p = rest.split("//").next()?.trim().trim_matches('"');
        (!p.is_empty() && rest.starts_with(char::is_whitespace)).then(|| p.to_string())
    })
}

fn work_uses(go_work: &str) -> Vec<String> {
    let mut out = vec![];
    let mut block = false;
    for l in go_work.lines() {
        let l = l.split("//").next().unwrap_or("").trim();
        let item = if block {
            if l.starts_with(')') {
                block = false;
                continue;
            }
            l
        } else if let Some(rest) = l.strip_prefix("use") {
            let rest = rest.trim();
            match rest.strip_prefix('(') {
                Some(r) => {
                    block = true;
                    r.trim()
                }
                None => rest,
            }
        } else {
            continue;
        };
        let item = item.trim_matches('"');
        if !item.is_empty() && !item.starts_with("..") {
            out.push(normalize(item));
        }
    }
    out
}

fn is_major(p: &str) -> bool {
    p.strip_prefix('v')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

fn third_party_module(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut n = match parts[0] {
        "github.com" | "gitlab.com" | "bitbucket.org" | "codeberg.org" | "golang.org" => 3,
        _ => 2,
    };
    if path.starts_with("github.com/aws/aws-sdk-go-v2/service/") {
        n = 5;
    }
    let n = n.min(parts.len());
    let n = n + usize::from(parts.get(n).is_some_and(|p| is_major(p)));
    parts[..n].join("/")
}

fn default_name(path: &str) -> &str {
    let mut parts = path.rsplit('/');
    let last = parts.next().unwrap_or(path);
    let last = match is_major(last) {
        true => parts.next().unwrap_or(last),
        false => last,
    };
    let last = last.strip_prefix("go-").unwrap_or(last);
    let last = last.strip_suffix("-go").unwrap_or(last);
    last.split_once(".v").map_or(last, |(n, _)| n)
}

fn left_out(src: &str) -> bool {
    let header = src
        .lines()
        .take_while(|l| !l.trim_start().starts_with("package "));
    header.map(str::trim).any(|l| {
        let constraint = l
            .strip_prefix("//go:build ")
            .or_else(|| l.strip_prefix("// +build "));
        constraint.is_some_and(|c| c.split_whitespace().any(|t| t == "ignore"))
            || (l.starts_with("// Code generated ") && l.ends_with("DO NOT EDIT."))
    })
}

fn package_clause(src: &str) -> Option<String> {
    src.lines()
        .find_map(|l| l.trim().strip_prefix("package "))
        .map(|p| p.trim().to_string())
}

impl LanguageFrontend for GoFrontend {
    fn handles(&self, path: &str, _: &Config) -> bool {
        path.ends_with(".go")
    }

    fn found(&self, files: &dyn Files, config: &Config) -> Vec<Package> {
        let ignore = globset(&config.ignore).ok();
        let skipped = |p: &str| {
            let hidden = dir_of(p).split('/').any(|d| {
                d != "." && (d.starts_with(['.', '_']) || d == "testdata" || d == "vendor")
            });
            let ignored = std::iter::successors(Some(p), |p| p.rsplit_once('/').map(|(d, _)| d))
                .any(|p| ignore.as_ref().is_some_and(|i| i.is_match(p)));
            hidden || ignored
        };
        let mut modules: Vec<(String, String)> = vec![];
        let add = |modules: &mut Vec<(String, String)>, dir: &str| {
            let path = files
                .read(&join(dir, "go.mod"))
                .and_then(|t| module_path(&t));
            if let Some(path) = path.filter(|_| !modules.iter().any(|m| m.0 == dir)) {
                modules.push((dir.into(), path));
            }
        };
        let work = files.read("go.work");
        for d in work.iter().flat_map(|w| work_uses(w)) {
            add(&mut modules, &d);
        }
        let found = files.walk(&["**/go.mod".into(), "**/*.go".into()]);
        let (mods, sources): (Vec<String>, Vec<String>) = (found.unwrap_or_default().into_iter())
            .filter(|f| !skipped(f))
            .partition(|f| f.ends_with("go.mod"));
        for m in &mods {
            add(&mut modules, dir_of(m));
        }
        modules.sort_by_key(|m| std::cmp::Reverse(m.0.len()));
        let dirs: BTreeSet<&str> = (sources.iter())
            .filter(|f| !f.ends_with("_test.go"))
            .map(|f| dir_of(f))
            .collect();
        dirs.into_iter()
            .map(|dir| {
                let module = modules
                    .iter()
                    .find_map(|(m, path)| Some((path, under(dir, m)?)));
                let name = match module {
                    Some((path, rest)) if rest.is_empty() => path.clone(),
                    Some((path, rest)) => format!("{path}/{rest}"),
                    None if dir == "." => (sources.iter())
                        .filter(|f| !f.contains('/'))
                        .filter_map(|f| files.read(f))
                        .find_map(|s| (!left_out(&s)).then(|| package_clause(&s)).flatten())
                        .unwrap_or_else(|| "main".into()),
                    None => dir.into(),
                };
                Package {
                    name,
                    dir: dir.into(),
                }
            })
            .collect()
    }

    fn build(
        &self,
        files: &[(SourceFile, String)],
        ws: &WorkspaceInfo,
        config: &Config,
    ) -> FrontendOutput {
        let tests = globset(&config.tests).ok();
        let extracted: Vec<(&SourceFile, Extracted)> = files
            .iter()
            .map(|(f, src)| {
                let is_test = f.path.ends_with("_test.go")
                    || tests.as_ref().is_some_and(|t| t.is_match(&f.path));
                let pkg = ws.packages.get(f.package).map(|p| p.name.as_str());
                let mut x = extract::extract(&f.path, src, pkg, is_test);
                if left_out(src) {
                    x.nodes.truncate(1);
                    x.package = None;
                    x.imports.clear();
                    x.calls.clear();
                    x.qualified.clear();
                }
                (f, x)
            })
            .collect();
        let index = Index::new(ws, &extracted);
        let mut out = FrontendOutput::default();
        for (f, x) in &extracted {
            index.edges(&f.path, x, &mut out);
        }
        out.facts = extracted
            .into_iter()
            .map(|(f, x)| FileFacts {
                file: f.path.clone(),
                nodes: x.nodes,
                parse_error: x.parse_error,
                ..FileFacts::default()
            })
            .collect();
        out.facts.sort_by(|a, b| a.file.cmp(&b.file));
        out
    }

    fn prepare_checkout(
        &self,
        _: &WorkspaceInfo,
        _: &Path,
        _: &Config,
        _: bool,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

struct Index<'a> {
    dirs: HashMap<&'a str, &'a str>,
    clause: HashMap<&'a str, &'a str>,
    files: BTreeMap<&'a str, Vec<&'a str>>,
    quals: HashMap<(&'a str, &'a str), &'a str>,
    named: HashMap<&'a str, Vec<&'a str>>,
    left_out: HashSet<&'a str>,
    ours: HashSet<String>,
}

impl<'a> Index<'a> {
    fn new(ws: &'a WorkspaceInfo, extracted: &'a [(&'a SourceFile, Extracted)]) -> Index<'a> {
        let go_dirs: HashSet<&str> = extracted.iter().map(|(f, _)| dir_of(&f.path)).collect();
        let dirs: HashMap<&str, &str> = (ws.packages.iter())
            .filter(|p| go_dirs.contains(p.dir.as_str()))
            .map(|p| (p.name.as_str(), p.dir.as_str()))
            .collect();
        let mut ix = Index {
            ours: dirs.keys().map(|d| third_party_module(d)).collect(),
            dirs,
            clause: HashMap::new(),
            files: BTreeMap::new(),
            quals: HashMap::new(),
            named: HashMap::new(),
            left_out: HashSet::new(),
        };
        for (f, x) in extracted {
            let dir = dir_of(&f.path);
            let test = f.path.ends_with("_test.go");
            if x.package.is_none() {
                ix.left_out.insert(&f.path);
            }
            if !test {
                ix.files.entry(dir).or_default().push(&f.path);
                if let Some(p) = &x.package {
                    ix.clause.entry(dir).or_insert(p);
                }
            }
            for n in x.nodes.iter().filter(|n| n.kind != "module") {
                let Some((_, q)) = n.id.split_once('#') else {
                    continue;
                };
                if !test || !ix.quals.contains_key(&(dir, q)) {
                    ix.quals.insert((dir, q), &n.id);
                }
                if matches!(n.kind.as_str(), "function" | "method") {
                    ix.named.entry(&n.name).or_default().push(&n.id);
                }
            }
        }
        ix
    }

    fn bindings(&self, x: &'a Extracted) -> (HashMap<String, &'a str>, Vec<&'a str>) {
        let mut named = HashMap::new();
        let mut dots = vec![];
        for i in &x.imports {
            let Some(&dir) = self.dirs.get(i.path.as_str()) else {
                continue;
            };
            match &i.local {
                Local::Named(n) => {
                    named.insert(n.clone(), dir);
                }
                Local::Default => {
                    let last = i.path.rsplit('/').next().unwrap_or(&i.path);
                    let n = self.clause.get(dir).copied().unwrap_or(last);
                    named.insert(n.to_string(), dir);
                }
                Local::Dot => dots.push(dir),
                Local::Blank => {}
            }
        }
        (named, dots)
    }

    fn guess(&self, name: &str, seen: &[&str]) -> Option<(String, &'static str)> {
        let ids: Vec<&&str> = (self.named.get(name).into_iter().flatten())
            .filter(|id| seen.contains(&dir_of(id.split('#').next().unwrap_or(id))))
            .collect();
        match ids[..] {
            [one] => Some((one.to_string(), "guessed")),
            _ => None,
        }
    }

    fn edges(&self, file: &str, x: &'a Extracted, out: &mut FrontendOutput) {
        let edge = |from: &str, to: String, kind: &str, confidence: &str, line, in_loop| Edge {
            from: from.into(),
            to,
            kind: kind.into(),
            confidence: confidence.into(),
            file: file.into(),
            line,
            namespace: false,
            reexport: false,
            in_loop,
        };
        let me = dir_of(file);
        let (bound, dots) = self.bindings(x);
        let mut seen = vec![me];
        seen.extend(bound.values().copied());
        seen.extend(dots.iter().copied());
        let outside: HashSet<&str> = (x.imports.iter())
            .filter(|i| !self.dirs.contains_key(i.path.as_str()))
            .filter_map(|i| match &i.local {
                Local::Named(n) => Some(n.as_str()),
                Local::Default => Some(default_name(&i.path)),
                _ => None,
            })
            .collect();
        let mut linked = HashSet::new();
        for i in &x.imports {
            let Some(&dir) = self.dirs.get(i.path.as_str()) else {
                let first = i.path.split('/').next().unwrap_or("");
                let module = third_party_module(&i.path);
                if first.contains('.') && !self.ours.contains(&module) {
                    out.packages.push((file.into(), module, i.line));
                }
                continue;
            };
            let local = bound
                .iter()
                .find(|(_, d)| **d == dir)
                .map(|(n, _)| n.as_str());
            let used: BTreeSet<&str> = (x.qualified.iter())
                .filter(|(p, _, _)| Some(p.as_str()) == local)
                .filter_map(|(_, name, _)| self.quals.get(&(dir, name.as_str())))
                .map(|id| id.split('#').next().unwrap_or(id))
                .collect();
            let targets: Vec<&str> = match used.is_empty() {
                true => {
                    let all = self.files.get(dir).cloned().unwrap_or_default();
                    let kept: Vec<&str> = (all.iter().copied())
                        .filter(|f| !self.left_out.contains(f))
                        .collect();
                    if kept.is_empty() { all } else { kept }
                }
                false => used.into_iter().collect(),
            };
            for t in targets.into_iter().filter(|t| *t != file) {
                if linked.insert(t) {
                    out.edges
                        .push(edge(file, t.into(), "imports", "resolved", i.line, false));
                }
            }
        }
        for r in &x.calls {
            let at = |dir: &str, q: &str| {
                self.quals
                    .get(&(dir, q))
                    .map(|id| (id.to_string(), "resolved"))
            };
            let to = match &r.callee {
                Callee::Name(n) => at(me, n).or_else(|| dots.iter().find_map(|d| at(d, n))),
                Callee::Qualified(p, n) => match bound.get(p) {
                    Some(dir) => at(dir, n),
                    None if outside.contains(p.as_str()) => None,
                    None => self.guess(n, &seen),
                },
                Callee::Member(ty, m) => {
                    at(me, &format!("{ty}.{m}")).or_else(|| self.guess(m, &seen))
                }
                Callee::Guess(n) => self.guess(n, &seen),
            };
            if let Some((to, conf)) = to
                && to != r.from
            {
                out.edges
                    .push(edge(&r.from, to, "calls", conf, r.line, r.in_loop));
            }
        }
    }
}

#[cfg(test)]
mod tests;
