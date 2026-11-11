//! The Rust frontend: tree-sitter extraction (modules, structs, enums, traits, functions, methods
//! as `Type.method`, `#[test]`s), then paths resolved against the crates of the tree: modules from
//! the file layout (`src/lib.rs`, `a.rs`, `a/mod.rs`), `crate::`/`self::`/`super::`, `use`
//! bindings and re-exports, other workspace crates; calls, `self.`/`Self::` methods and trait impls.

mod extract;

use extract::{Callee, Extracted, Path as RsPath};
use ig_core::config::Config;
use ig_core::model::Edge;
use ig_core::ports::{FileFacts, Files, FrontendOutput, LanguageFrontend, PortError};
use ig_core::workspace::{Package, SourceFile, WorkspaceInfo, globset};
use std::collections::{HashMap, HashSet};
use std::path::Path;

pub struct RsFrontend;

/// The crates the language ships: no package.
const BUILTIN: [&str; 5] = ["std", "core", "alloc", "proc_macro", "test"];

fn toml_name(t: &toml::Table) -> Option<String> {
    t.get("package")?.get("name")?.as_str().map(String::from)
}

impl LanguageFrontend for RsFrontend {
    fn handles(&self, path: &str, _: &Config) -> bool {
        path.ends_with(".rs")
    }

    /// The root `Cargo.toml`'s package, and its workspace `members` (minus `exclude`).
    fn packages(&self, files: &dyn Files, _: &Config) -> Vec<Package> {
        let read = |p: &str| files.read(p).and_then(|t| t.parse::<toml::Table>().ok());
        let Some(root) = read("Cargo.toml") else {
            return vec![];
        };
        let mut out: Vec<Package> = toml_name(&root)
            .map(|name| Package {
                name,
                dir: ".".into(),
            })
            .into_iter()
            .collect();
        let Some(ws) = root.get("workspace").and_then(|w| w.as_table()) else {
            return out;
        };
        let list = |k: &str| -> Vec<String> {
            let v = ws.get(k).and_then(|v| v.as_array());
            let clean = |s: &str| s.trim_start_matches("./").trim_end_matches('/').to_string();
            v.into_iter()
                .flatten()
                .filter_map(|s| s.as_str().map(clean))
                .collect()
        };
        let exclude = globset(&list("exclude")).ok();
        let manifests: Vec<String> = list("members")
            .iter()
            .map(|m| format!("{m}/Cargo.toml"))
            .collect();
        for m in files.glob(&manifests).unwrap_or_default() {
            let dir = m.strip_suffix("/Cargo.toml").unwrap_or(".");
            if exclude.as_ref().is_some_and(|e| e.is_match(dir)) || out.iter().any(|p| p.dir == dir)
            {
                continue;
            }
            let name = read(&m).and_then(|t| toml_name(&t));
            out.push(Package {
                name: name.unwrap_or_else(|| dir.rsplit('/').next().unwrap_or(dir).into()),
                dir: dir.into(),
            });
        }
        out
    }

    /// A folder with its own `Cargo.toml` that no workspace lists is another project.
    fn fences(&self) -> &'static [&'static str] {
        &["Cargo.toml"]
    }

    fn build(
        &self,
        files: &[(SourceFile, String)],
        ws: &WorkspaceInfo,
        _: &Config,
    ) -> FrontendOutput {
        // ponytail: one file after another; rayon (as TypeScript's) when large trees need it
        let extracted: Vec<(&SourceFile, Extracted)> = files
            .iter()
            .map(|(f, src)| {
                let pkg = ws.packages.get(f.package).map(|p| p.name.as_str());
                (f, extract::extract(&f.path, src, pkg))
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

    /// Rust is read from source alone: nothing to link.
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

/// A file's place: its crate (a package index) and module path.
type Place = (usize, Vec<String>);

struct Index<'a> {
    /// Crate name as code writes it (`ig-core` → `ig_core`) → package index.
    crates: HashMap<String, usize>,
    /// (crate, `a::b`) → file; a crate's root is "".
    modules: HashMap<(usize, String), &'a str>,
    places: HashMap<&'a str, Place>,
    ids: HashSet<&'a str>,
    /// Node id → kind; `Type.method` → ids; a name → ids of functions and methods.
    kinds: HashMap<&'a str, &'a str>,
    quals: HashMap<String, Vec<&'a str>>,
    named: HashMap<&'a str, Vec<&'a str>>,
    uses: HashMap<&'a str, HashMap<&'a str, &'a RsPath>>,
    globs: HashMap<&'a str, Vec<&'a RsPath>>,
    /// Type name → the traits it implements, as written.
    traits: HashMap<String, Vec<String>>,
    /// Per crate, the crates it can see: itself and the workspace crates its files `use`.
    sees: HashMap<usize, HashSet<usize>>,
}

fn place(dir: &str, file: &str) -> Option<Vec<String>> {
    let rel = if dir == "." {
        file
    } else {
        file.strip_prefix(&format!("{dir}/"))?
    };
    let Some(s) = rel.strip_prefix("src/") else {
        // tests/, examples/, benches/, build.rs: crates of their own, never named by a path
        return Some(vec![format!("#{rel}")]);
    };
    if s == "lib.rs" || s == "main.rs" {
        return Some(vec![]);
    }
    let s = s.strip_suffix(".rs").unwrap_or(s);
    let s = s.strip_suffix("/mod").unwrap_or(s);
    Some(s.split('/').map(String::from).collect())
}

impl<'a> Index<'a> {
    fn new(ws: &'a WorkspaceInfo, extracted: &'a [(&'a SourceFile, Extracted)]) -> Index<'a> {
        let mut ix = Index {
            crates: (ws.packages.iter().enumerate())
                .map(|(i, p)| (p.name.replace('-', "_"), i))
                .collect(),
            modules: HashMap::new(),
            places: HashMap::new(),
            ids: HashSet::new(),
            kinds: HashMap::new(),
            quals: HashMap::new(),
            named: HashMap::new(),
            uses: HashMap::new(),
            globs: HashMap::new(),
            traits: HashMap::new(),
            sees: HashMap::new(),
        };
        // lib.rs before main.rs: the library is the crate other code names
        let mut files: Vec<&SourceFile> = extracted.iter().map(|(f, _)| *f).collect();
        files.sort_by_key(|f| (!f.path.ends_with("src/lib.rs"), f.path.as_str()));
        for f in files {
            let Some(p) = ws.packages.get(f.package) else {
                continue;
            };
            let Some(m) = place(&p.dir, &f.path) else {
                continue;
            };
            ix.modules
                .entry((f.package, m.join("::")))
                .or_insert(&f.path);
            ix.places.insert(&f.path, (f.package, m));
        }
        for (f, x) in extracted {
            for n in &x.nodes {
                ix.ids.insert(&n.id);
                ix.kinds.insert(&n.id, &n.kind);
                if n.kind == "module" {
                    continue;
                }
                if let Some((_, q)) = n.id.split_once('#') {
                    ix.quals.entry(q.to_string()).or_default().push(&n.id);
                }
                if n.kind != "class" {
                    ix.named.entry(&n.name).or_default().push(&n.id);
                }
            }
            ix.uses.insert(
                &f.path,
                x.uses.iter().map(|(n, p, _)| (n.as_str(), p)).collect(),
            );
            ix.globs
                .insert(&f.path, x.globs.iter().map(|(p, _)| p).collect());
            for (ty, tr, _) in &x.impls {
                let tr = match tr {
                    Callee::Path(p) => p.last().cloned().unwrap_or_default(),
                    Callee::Name(n) => n.clone(),
                    _ => continue,
                };
                let Callee::Name(ty) = ty else { continue };
                ix.traits.entry(ty.clone()).or_default().push(tr);
            }
        }
        for (f, x) in extracted {
            let Some(&(c, _)) = ix.places.get(f.path.as_str()) else {
                continue;
            };
            let used = (x
                .uses
                .iter()
                .map(|u| &u.1)
                .chain(x.globs.iter().map(|g| &g.0)))
            .filter_map(|p| ix.crates.get(p[0].as_str()).copied());
            ix.sees
                .entry(c)
                .or_insert_with(|| HashSet::from([c]))
                .extend(used);
        }
        ix
    }

    /// The node `path` names as `file` writes it: a module (its file) or an item in one.
    fn path(&self, file: &str, path: &[String], hops: u8) -> Option<String> {
        let (ci, m) = self.places.get(file)?.clone();
        let first = path.first()?.as_str();
        let child = |c: usize, mut at: Vec<String>, s: &str| {
            at.push(s.into());
            self.modules.contains_key(&(c, at.join("::")))
        };
        let (mut c, mut cur, mut i) = (ci, m.clone(), 0);
        match first {
            "crate" => (cur, i) = (vec![], 1),
            "self" => i = 1,
            "super" => {
                while path.get(i).is_some_and(|s| s == "super") {
                    cur.pop();
                    i += 1;
                }
            }
            _ => {
                let bound = self
                    .uses
                    .get(file)
                    .and_then(|u| u.get(first))
                    .filter(|b| b.len() > 1 || b[0] != first);
                if let Some(b) = bound.filter(|_| hops > 0) {
                    return self.path(file, &[&b[..], &path[1..]].concat(), hops - 1);
                }
                if child(ci, m.clone(), first) {
                } else if let Some(&k) = self.crates.get(first) {
                    (c, cur, i) = (k, vec![], 1);
                } else if child(ci, vec![], first) {
                    cur = vec![];
                } else {
                    return None;
                }
            }
        }
        while i < path.len() && child(c, cur.clone(), &path[i]) {
            cur.push(path[i].clone());
            i += 1;
        }
        let at = *self.modules.get(&(c, cur.join("::")))?;
        let rest = &path[i..];
        if rest.is_empty() {
            return Some(at.into());
        }
        let id = format!("{at}#{}", rest.join("."));
        if self.ids.contains(id.as_str()) {
            return Some(id);
        }
        // `pub use` re-exports, and `use x::*`, of the module named
        if hops == 0 {
            return None;
        }
        if let Some(b) = self.uses.get(at).and_then(|u| u.get(rest[0].as_str())) {
            return self.path(at, &[&b[..], &rest[1..]].concat(), hops - 1);
        }
        (self.globs.get(at).into_iter().flatten())
            .find_map(|g| self.path(at, &[&g[..], rest].concat(), hops - 1))
    }

    /// The one function or method called `name` in a crate `file`'s crate can see.
    fn guess(&self, file: &str, name: &str) -> Option<(String, &'static str)> {
        let sees = self.places.get(file).and_then(|p| self.sees.get(&p.0));
        let seen = |id: &&str| {
            let c = self
                .places
                .get(id.split('#').next().unwrap_or(id))
                .map(|p| p.0);
            c.zip(sees).is_some_and(|(c, s)| s.contains(&c))
        };
        let ids: Vec<&str> = self
            .named
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .filter(seen)
            .collect();
        match ids[..] {
            [one] => Some((one.to_string(), "guessed")),
            _ => None,
        }
    }

    /// Method `m` of the type named `ty`: its own impl's, else a trait's it implements; the one
    /// in `file`'s crate when several types share the name.
    fn method(&self, file: &str, ty: &str, m: &str) -> Option<(String, &'static str)> {
        let crate_of = |id: &str| {
            self.places
                .get(id.split('#').next().unwrap_or(id))
                .map(|p| p.0)
        };
        let mine = self.places.get(file).map(|p| p.0);
        let own = std::iter::once(ty.to_string())
            .chain(self.traits.get(ty).into_iter().flatten().cloned());
        for t in own {
            let ids = self
                .quals
                .get(&format!("{t}.{m}"))
                .map(Vec::as_slice)
                .unwrap_or_default();
            let here: Vec<&str> = ids
                .iter()
                .copied()
                .filter(|id| crate_of(id) == mine)
                .collect();
            match (here.as_slice(), ids) {
                ([one], _) | ([], [one]) => return Some((one.to_string(), "resolved")),
                _ => {}
            }
        }
        None
    }

    fn target(&self, file: &str, callee: &Callee) -> Option<(String, &'static str)> {
        match callee {
            Callee::Name(n) => {
                let own = format!("{file}#{n}");
                if self.ids.contains(own.as_str()) {
                    return Some((own, "resolved"));
                }
                let bound = self.uses.get(file).and_then(|u| u.get(n.as_str()));
                (bound.and_then(|b| self.path(file, b, 3)))
                    .or_else(|| {
                        (self.globs.get(file).into_iter().flatten()).find_map(|g| {
                            self.path(file, &[&g[..], std::slice::from_ref(n)].concat(), 3)
                        })
                    })
                    .map(|t| (t, "resolved"))
                    .or_else(|| self.guess(file, n))
            }
            Callee::Path(p) => {
                if let Some(t) = self.path(file, p, 3) {
                    return Some((t, "resolved"));
                }
                // `Type::f()` / `a::Type::f()`: a method of that type
                let (m, ty) = p.split_last()?;
                let ty_id = match ty {
                    [one] => self.target(file, &Callee::Name(one.clone())).map(|t| t.0),
                    _ => self.path(file, ty, 3),
                }?;
                if self.kinds.get(ty_id.as_str()) != Some(&"class") {
                    return None;
                }
                let name = ty_id.rsplit(['#', '.']).next().unwrap_or(&ty_id);
                self.method(&ty_id, name, m)
            }
            Callee::Member(ty, m) => self.method(file, ty, m).or_else(|| self.guess(file, m)),
            Callee::Guess(m) => self.guess(file, m),
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
        let module_of = |id: &str| id.split('#').next().unwrap_or(id).to_string();
        let mut seen = HashSet::new();
        for (name, line) in &x.mods {
            let Some((c, m)) = self.places.get(file) else {
                continue;
            };
            let sub = [&m[..], std::slice::from_ref(name)].concat().join("::");
            if let Some(t) = self.modules.get(&(*c, sub)) {
                out.edges.push(edge(
                    file,
                    t.to_string(),
                    "imports",
                    "resolved",
                    *line,
                    false,
                ));
            }
        }
        let paths =
            (x.uses.iter().map(|(_, p, l)| (p, *l))).chain(x.globs.iter().map(|(p, l)| (p, *l)));
        for (p, line) in paths {
            match self.path(file, p, 3) {
                Some(t) if module_of(&t) != file => {
                    let to = module_of(&t);
                    if seen.insert(to.clone()) {
                        out.edges
                            .push(edge(file, to, "imports", "resolved", line, false));
                    }
                }
                Some(_) => {}
                None => {
                    let first = p[0].as_str();
                    let local = ["crate", "self", "super"].contains(&first)
                        || self.crates.contains_key(first)
                        || self
                            .uses
                            .get(file)
                            .is_some_and(|u| u.contains_key(first) && p.len() > 1);
                    if !local && !BUILTIN.contains(&first) {
                        out.packages.push((file.into(), first.into(), line));
                    }
                }
            }
        }
        for r in &x.calls {
            if let Some((to, conf)) = self.target(file, &r.callee)
                && to != r.from
            {
                out.edges
                    .push(edge(&r.from, to, "calls", conf, r.line, r.in_loop));
            }
        }
        for (ty, tr, line) in &x.impls {
            let (Some((t, _)), Some((tr, conf))) = (self.target(file, ty), self.target(file, tr))
            else {
                continue;
            };
            if self.kinds.get(t.as_str()) == Some(&"class")
                && self.kinds.get(tr.as_str()) == Some(&"class")
            {
                out.edges
                    .push(edge(&t, tr, "implements", conf, *line, false));
            }
        }
    }
}

#[cfg(test)]
mod tests;
