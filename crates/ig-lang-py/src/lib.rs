//! The Python frontend: tree-sitter extraction (modules, classes, functions, methods, tests), then
//! imports resolved against the tree's own modules (absolute from each package's folder or its
//! `src/`, and relative), calls through those imports, `self.` methods and base classes.

mod extract;
mod stdlib;

use extract::{Callee, Extracted, Ref};
use ig_core::config::Config;
use ig_core::model::Edge;
use ig_core::ports::{FileFacts, Files, FrontendOutput, LanguageFrontend, PortError};
use ig_core::workspace::{Package, SourceFile, WorkspaceInfo, globset};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

pub struct PyFrontend;

static NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?m)^\s*name\s*=\s*["']([^"']+)["']|\bname\s*=\s*["']([^"']+)["']"#).unwrap()
});

/// A `pyproject.toml`'s `[project]` or `[tool.poetry]` name, or a `setup.py`'s `name=`.
fn manifest_name(text: &str, toml: bool) -> Option<String> {
    if !toml {
        return NAME
            .captures(text)
            .and_then(|c| c.get(2).or(c.get(1)))
            .map(|m| m.as_str().into());
    }
    let mut section = "";
    for l in text.lines().map(str::trim) {
        if l.starts_with('[') {
            section = l;
        } else if (section == "[project]" || section == "[tool.poetry]")
            && let Some(c) = NAME.captures(l)
        {
            return c.get(1).map(|m| m.as_str().into());
        }
    }
    None
}

impl LanguageFrontend for PyFrontend {
    fn handles(&self, path: &str, _: &Config) -> bool {
        path.to_ascii_lowercase().ends_with(".py")
    }

    /// Every folder with a `pyproject.toml` or `setup.py`, at most two folders down.
    // ponytail: two levels covers services/x and packages/x; deeper Python projects join their parent
    fn packages(&self, files: &dyn Files) -> Vec<Package> {
        let manifests: Vec<String> = ["", "*/", "*/*/"]
            .iter()
            .flat_map(|d| [format!("{d}pyproject.toml"), format!("{d}setup.py")])
            .collect();
        let mut out: Vec<Package> = vec![];
        for m in files.glob(&manifests).unwrap_or_default() {
            let (dir, file) = m.rsplit_once('/').unwrap_or((".", &m));
            if out.iter().any(|p| p.dir == dir) {
                continue;
            }
            let named = files
                .read(&m)
                .and_then(|t| manifest_name(&t, file == "pyproject.toml"));
            out.push(Package {
                name: named.unwrap_or_else(|| dir.rsplit('/').next().unwrap_or(dir).into()),
                dir: dir.into(),
            });
        }
        out
    }

    fn build(
        &self,
        files: &[(SourceFile, String)],
        ws: &WorkspaceInfo,
        config: &Config,
    ) -> FrontendOutput {
        let tests = globset(&config.tests).ok();
        // ponytail: one file after another; rayon (as TypeScript's) when large trees need it
        let extracted: Vec<(&SourceFile, Extracted)> = files
            .iter()
            .map(|(f, src)| {
                let is_test = tests.as_ref().is_some_and(|t| t.is_match(&f.path));
                let pkg = ws.packages.get(f.package).map(|p| p.name.as_str());
                (f, extract::extract(&f.path, src, pkg, is_test))
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

    /// Python is read from source alone: nothing to link.
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

/// What a name a file imports stands for.
enum Bound<'a> {
    /// A module, by its dotted name (relative ones keep their dots).
    Module(String),
    /// Name `.1` of module `.0`.
    Name(String, &'a str),
}

struct Index<'a> {
    /// Every Python file of the tree.
    files: HashSet<&'a str>,
    /// Dotted module name → file, from the tree's root, each package's folder and their `src/`.
    modules: HashMap<String, &'a str>,
    /// Node ids, and the ids of each name.
    ids: HashSet<&'a str>,
    named: HashMap<&'a str, Vec<&'a str>>,
    /// Per file: the names it imports.
    bound: HashMap<&'a str, HashMap<&'a str, Bound<'a>>>,
    /// Per class: the classes it extends, as written.
    bases: HashMap<&'a str, Vec<&'a Callee>>,
}

impl<'a> Index<'a> {
    fn new(ws: &'a WorkspaceInfo, extracted: &'a [(&'a SourceFile, Extracted)]) -> Index<'a> {
        let files: HashSet<&str> = (ws.files.iter())
            .map(|f| f.path.as_str())
            .filter(|p| p.ends_with(".py"))
            .collect();
        let mut roots: Vec<String> = vec![".".into(), "src".into()];
        for p in &ws.packages {
            roots.extend([p.dir.clone(), format!("{}/src", p.dir)]);
        }
        roots.retain(|r| r != "./src");
        // the deepest root names a module first
        roots.sort_by_key(|r| std::cmp::Reverse(r.matches('/').count() + usize::from(r != ".")));
        roots.dedup();
        let mut modules = HashMap::new();
        for f in &files {
            for r in &roots {
                let rel = if r == "." {
                    Some(*f)
                } else {
                    f.strip_prefix(&format!("{r}/"))
                };
                let Some(rel) = rel else { continue };
                let stem = rel.strip_suffix(".py").unwrap_or(rel);
                let stem = stem.strip_suffix("/__init__").unwrap_or(stem);
                if stem != "__init__" {
                    modules.entry(stem.replace('/', ".")).or_insert(*f);
                }
            }
        }
        let mut ix = Index {
            files,
            modules,
            ids: HashSet::new(),
            named: HashMap::new(),
            bound: HashMap::new(),
            bases: HashMap::new(),
        };
        for (_, x) in extracted {
            for n in &x.nodes {
                ix.ids.insert(&n.id);
                if n.kind != "module" {
                    ix.named.entry(&n.name).or_default().push(&n.id);
                }
            }
            for r in x.refs.iter().filter(|r| r.kind == "extends") {
                ix.bases.entry(&r.from).or_default().push(&r.callee);
            }
        }
        for (f, x) in extracted {
            let mut b = HashMap::new();
            for i in &x.imports {
                let Some(name) = i.name.as_deref().filter(|n| *n != "*") else {
                    if i.name.is_none() {
                        let m = if i.local == i.module
                            || !i.module.starts_with(&format!("{}.", i.local))
                        {
                            i.module.clone()
                        } else {
                            i.local.clone()
                        };
                        b.insert(i.local.as_str(), Bound::Module(m));
                    }
                    continue;
                };
                let sub = join(&i.module, name);
                let bound = match ix.module(&f.path, &sub) {
                    Some(_) => Bound::Module(sub),
                    None => Bound::Name(i.module.clone(), name),
                };
                b.insert(i.local.as_str(), bound);
            }
            ix.bound.insert(&f.path, b);
        }
        ix
    }

    /// The file of module `spec` as `file` imports it.
    fn module(&self, file: &str, spec: &str) -> Option<&'a str> {
        let dots = spec.len() - spec.trim_start_matches('.').len();
        if dots == 0 {
            return self.modules.get(spec).copied();
        }
        let mut dir = file.rsplit_once('/').map_or("", |(d, _)| d);
        for _ in 1..dots {
            dir = dir.rsplit_once('/').map_or("", |(d, _)| d);
        }
        let rest = spec[dots..].replace('.', "/");
        let base = [dir, rest.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("/");
        let at = |p: String| self.files.get(p.trim_start_matches('/')).copied();
        match rest.is_empty() {
            true => at(format!("{base}/__init__.py")),
            false => at(format!("{base}.py")).or_else(|| at(format!("{base}/__init__.py"))),
        }
    }

    /// Name `name` of the module in `file`: its own, else one it imports (a few hops at most).
    fn name_in(&self, file: &str, name: &str, hops: u8) -> Option<String> {
        let own = format!("{file}#{name}");
        if self.ids.contains(own.as_str()) {
            return Some(own);
        }
        match self.bound.get(file)?.get(name)? {
            Bound::Module(m) => self.module(file, m).map(str::to_string),
            Bound::Name(m, n) if hops > 0 => self.name_in(self.module(file, m)?, n, hops - 1),
            Bound::Name(..) => None,
        }
    }

    /// The one class, function or method called `name` in the tree.
    fn guess(&self, name: &str) -> Option<(String, &'static str)> {
        match self.named.get(name).map(Vec::as_slice) {
            Some([one]) => Some((one.to_string(), "guessed")),
            _ => None,
        }
    }

    fn target(&self, file: &str, callee: &Callee, hops: u8) -> Option<(String, &'static str)> {
        match callee {
            Callee::Name(n) => self
                .name_in(file, n, 3)
                .map(|t| (t, "resolved"))
                .or_else(|| self.guess(n)),
            Callee::Path(chain, attr) => {
                // the longest leading part that is a module: the rest is a name inside it
                let bound = self.bound.get(file).and_then(|b| b.get(chain[0].as_str()));
                let head = match bound {
                    Some(Bound::Module(m)) => Some(m.clone()),
                    Some(Bound::Name(m, n)) => Some(join(m, n)),
                    None => None,
                };
                if let Some(head) = head {
                    let mut parts: Vec<&str> = chain[1..].iter().map(String::as_str).collect();
                    parts.push(attr);
                    for k in (0..parts.len()).rev() {
                        let spec = std::iter::once(head.as_str())
                            .chain(parts[..k].iter().copied())
                            .collect::<Vec<_>>();
                        let spec = spec.iter().fold(String::new(), |s, p| {
                            if s.is_empty() {
                                p.to_string()
                            } else {
                                join(&s, p)
                            }
                        });
                        if let Some(m) = self.module(file, &spec) {
                            let id = format!("{m}#{}", parts[k..].join("."));
                            if self.ids.contains(id.as_str()) {
                                return Some((id, "resolved"));
                            }
                        }
                    }
                }
                self.guess(attr)
            }
            Callee::Member(class, m) => {
                let id = format!("{class}.{m}");
                if self.ids.contains(id.as_str()) {
                    return Some((id, "resolved"));
                }
                let base = (self.bases.get(class.as_str()).into_iter().flatten())
                    .filter(|_| hops > 0)
                    .filter_map(|b| self.target(file, b, 0))
                    .find_map(|(b, _)| self.target(file, &Callee::Member(b, m.clone()), hops - 1));
                base.or_else(|| self.guess(m))
            }
            Callee::Guess(n) => self.guess(n),
        }
    }

    fn edges(&self, file: &str, x: &Extracted, out: &mut FrontendOutput) {
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
        for i in &x.imports {
            if i.module.starts_with("__") {
                continue;
            }
            let full = match i.name.as_deref() {
                Some(n) if n != "*" => join(&i.module, n),
                _ => i.module.clone(),
            };
            let to = self
                .module(file, &full)
                .or_else(|| self.module(file, &i.module));
            // `import a.b.c` with only `a.b` in the tree: the deepest module there is
            let to = to.or_else(|| {
                let mut s = i.module.as_str();
                while let Some((head, _)) = s.rsplit_once('.') {
                    s = head;
                    if let Some(m) = self.module(file, s) {
                        return Some(m);
                    }
                }
                None
            });
            match to {
                Some(t) if t != file => {
                    out.edges
                        .push(edge(file, t.into(), "imports", "resolved", i.line, false))
                }
                Some(_) => {}
                None if !i.module.starts_with('.') => {
                    let top = i.module.split('.').next().unwrap_or(&i.module);
                    if !stdlib::STDLIB.contains(&top) && !self.modules.contains_key(top) {
                        out.packages.push((file.into(), top.into(), i.line));
                    }
                }
                None => {}
            }
        }
        for Ref {
            from,
            kind,
            callee,
            line,
            in_loop,
        } in &x.refs
        {
            if let Some((to, conf)) = self.target(file, callee, 3)
                && to != *from
            {
                out.edges.push(edge(from, to, kind, conf, *line, *in_loop));
            }
        }
    }
}

/// `module.name`, or `..name` when `module` is only dots.
fn join(module: &str, name: &str) -> String {
    if module.is_empty() || module.chars().all(|c| c == '.') {
        format!("{module}{name}")
    } else {
        format!("{module}.{name}")
    }
}

#[cfg(test)]
mod tests;
