//! Pass 1: one Rust file's nodes (module, structs, enums, traits, functions, methods, tests), its
//! `use` bindings and `mod` declarations, and the calls and trait impls still to resolve.

use ig_core::model::{Metrics, Node, Range, innermost_at, intents_in, normalized_hash, sha1_bytes};
use std::collections::HashMap;
use tree_sitter::{Node as Ts, Parser};

/// A path as written: `crate::a::B` is ["crate", "a", "B"].
pub type Path = Vec<String>;

#[derive(Debug, Clone)]
pub enum Callee {
    /// `f()` or `B`: a name of this file or one it brings in with `use`.
    Name(String),
    /// `a::b::f()`.
    Path(Path),
    /// `self.m()` or `Self::m()` inside `impl Type`.
    Member(String, String),
    /// `x.m()`: matched by name.
    Guess(String),
}

#[derive(Debug, Clone)]
pub struct Ref {
    pub from: String,
    pub callee: Callee,
    pub line: u32,
    pub in_loop: bool,
}

#[derive(Debug, Default)]
pub struct Extracted {
    pub nodes: Vec<Node>,
    pub parse_error: Option<String>,
    /// Local name → the path it stands for (`use a::b as c` binds "c"); `*` paths in `globs`.
    pub uses: Vec<(String, Path, u32)>,
    pub globs: Vec<(Path, u32)>,
    /// `mod x;`: (name, line), a module in a file of its own.
    pub mods: Vec<(String, u32)>,
    pub calls: Vec<Ref>,
    /// `impl Trait for Type`: (type, trait, line).
    pub impls: Vec<(Callee, Callee, u32)>,
}

struct Scope {
    qual: String,
    id: String,
}

struct X<'s> {
    src: &'s str,
    file: &'s str,
    package: Option<&'s str>,
    out: Extracted,
    ids: HashMap<String, u32>,
    scopes: Vec<Scope>,
    /// The type of the `impl` block around this point.
    impl_type: Option<String>,
    /// The inline modules (`mod m { … }`) around this point.
    inline: Vec<String>,
}

fn line(n: Ts) -> u32 {
    n.start_position().row as u32 + 1
}

/// Decision points under `n`, not counting nested items.
fn decisions(n: Ts, src: &str) -> u32 {
    let mut c = n.walk();
    n.named_children(&mut c)
        .map(|k| match k.kind() {
            "function_item" | "impl_item" | "mod_item" | "trait_item" => 0,
            "if_expression" | "match_arm" | "for_expression" | "while_expression"
            | "loop_expression" | "try_expression" => 1 + decisions(k, src),
            "binary_expression"
                if k.child_by_field_name("operator")
                    .is_some_and(|o| matches!(&src[o.byte_range()], "&&" | "||")) =>
            {
                1 + decisions(k, src)
            }
            _ => decisions(k, src),
        })
        .sum()
}

/// The last type name of a type (`Vec<T>` → "Vec", `a::B` → "B", `&mut C` → "C").
fn type_name(t: Ts, src: &str) -> Option<String> {
    match t.kind() {
        "type_identifier" => Some(src[t.byte_range()].into()),
        "generic_type" => type_name(t.child_by_field_name("type")?, src),
        "scoped_type_identifier" => type_name(t.child_by_field_name("name")?, src),
        "reference_type" | "pointer_type" => type_name(t.child_by_field_name("type")?, src),
        _ => None,
    }
}

impl<'s> X<'s> {
    fn text(&self, n: Ts) -> &'s str {
        &self.src[n.byte_range()]
    }

    /// `p` as the file's own module would write it: `self::`/`super::` written inside inline
    /// modules start from the file's module.
    fn anchor(&self, p: Path) -> Path {
        let ups = p.iter().take_while(|s| *s == "super").count();
        if self.inline.is_empty() || (ups == 0 && p.first().is_none_or(|s| s != "self")) {
            return p;
        }
        let (d, skip) = (self.inline.len(), if ups == 0 { 1 } else { ups });
        let mut out = vec![];
        if ups > d {
            out.extend(std::iter::repeat_n("super".to_string(), ups - d));
        } else {
            out.push("self".into());
            out.extend(self.inline[..d - ups].iter().cloned());
        }
        out.extend(p.into_iter().skip(skip));
        out
    }

    /// The comments and attributes right above `item`: its `@intent` tags, and whether an
    /// attribute names `test`.
    fn above(&self, item: Ts) -> (Vec<String>, bool) {
        let (mut intents, mut test) = (vec![], false);
        let mut p = item.prev_sibling();
        while let Some(c) = p {
            match c.kind() {
                "line_comment" | "block_comment" => intents_in(self.text(c), &mut intents),
                "attribute_item" => {
                    let a = self.text(c);
                    test |= a.contains("test]") || a.contains("::test") || a == "#[test]";
                }
                _ => break,
            }
            p = c.prev_sibling();
        }
        (intents, test)
    }

    fn define(&mut self, item: Ts, kind: &str, name: &str, body: Option<Ts>) -> (String, String) {
        let parent = self.scopes.last();
        let qual = parent.map_or_else(|| name.to_string(), |s| format!("{}.{name}", s.qual));
        let base = format!("{}#{qual}", self.file);
        let n = self.ids.entry(base.clone()).or_insert(0);
        *n += 1;
        let (id, qual) = match *n {
            1 => (base, qual),
            n => (format!("{base}~{n}"), format!("{qual}~{n}")),
        };
        let (intents, test) = self.above(item);
        let kind = if test && kind != "class" {
            "test"
        } else {
            kind
        };
        let (start, end) = (line(item), item.end_position().row as u32 + 1);
        let mut c = item.walk();
        let public = item
            .children(&mut c)
            .any(|k| k.kind() == "visibility_modifier");
        self.out.nodes.push(Node {
            id: id.clone(),
            kind: kind.into(),
            name: name.into(),
            file: self.file.into(),
            range: Range {
                start_line: start,
                end_line: end,
            },
            parent: Some(parent.map_or_else(|| self.file.to_string(), |s| s.id.clone())),
            package: self.package.map(str::to_string),
            exported: public && kind != "test",
            hash: normalized_hash(self.text(item)),
            metrics: Metrics {
                loc: end - start + 1,
                complexity: 1 + body.map_or(0, |b| decisions(b, self.src)),
            },
            intents,
        });
        (id, qual)
    }

    /// The item at the end of a `use` tree: each name it binds, with its path.
    fn uses(&mut self, t: Ts, prefix: &Path, at: u32) {
        let seg = |n: Ts| -> Path {
            let mut out = vec![];
            let mut s = Some(n);
            while let Some(x) = s {
                match x.kind() {
                    "scoped_identifier" => {
                        if let Some(name) = x.child_by_field_name("name") {
                            out.push(self.text(name).to_string());
                        }
                        s = x.child_by_field_name("path");
                    }
                    _ => {
                        out.push(self.text(x).to_string());
                        s = None;
                    }
                }
            }
            out.reverse();
            out
        };
        let join = |p: Path| [prefix.clone(), p].concat();
        match t.kind() {
            "use_as_clause" => {
                let (Some(p), Some(a)) = (
                    t.child_by_field_name("path"),
                    t.child_by_field_name("alias"),
                ) else {
                    return;
                };
                let alias = self.text(a).to_string();
                let p = self.anchor(join(seg(p)));
                self.out.uses.push((alias, p, at));
            }
            "scoped_use_list" => {
                let p = t.child_by_field_name("path").map(&seg).unwrap_or_default();
                let p = join(p);
                if let Some(list) = t.child_by_field_name("list") {
                    self.uses(list, &p, at);
                }
            }
            "use_list" => {
                let mut c = t.walk();
                let kids: Vec<Ts> = t.named_children(&mut c).collect();
                for k in kids {
                    self.uses(k, prefix, at);
                }
            }
            "use_wildcard" => {
                let mut c = t.walk();
                let p = t
                    .named_children(&mut c)
                    .next()
                    .map(&seg)
                    .unwrap_or_default();
                let p = self.anchor(join(p));
                self.out.globs.push((p, at));
            }
            "self" => {
                if let Some(last) = prefix.last() {
                    let p = self.anchor(prefix.clone());
                    self.out.uses.push((last.clone(), p, at));
                }
            }
            _ => {
                let p = join(seg(t));
                if let Some(last) = p.last().cloned() {
                    let p = self.anchor(p);
                    self.out.uses.push((last, p, at));
                }
            }
        }
    }

    /// What an expression in call position names.
    fn callee(&self, f: Ts) -> Option<Callee> {
        match f.kind() {
            "identifier" => Some(Callee::Name(self.text(f).into())),
            "generic_function" => self.callee(f.child_by_field_name("function")?),
            "scoped_identifier" => {
                let mut p = vec![];
                let mut s = Some(f);
                while let Some(x) = s {
                    match x.kind() {
                        "scoped_identifier" => {
                            p.push(self.text(x.child_by_field_name("name")?).to_string());
                            s = x.child_by_field_name("path");
                        }
                        "generic_type" => {
                            p.push(self.text(x.child_by_field_name("type")?).to_string());
                            s = None;
                        }
                        _ => {
                            p.push(self.text(x).to_string());
                            s = None;
                        }
                    }
                }
                p.reverse();
                match (&p[..], &self.impl_type) {
                    ([me, m], Some(t)) if me == "Self" => {
                        Some(Callee::Member(t.clone(), m.clone()))
                    }
                    _ => Some(Callee::Path(p)),
                }
            }
            "field_expression" => {
                let m = self.text(f.child_by_field_name("field")?).to_string();
                let on_self = f
                    .child_by_field_name("value")
                    .is_some_and(|v| v.kind() == "self");
                match (&self.impl_type, on_self) {
                    (Some(t), true) => Some(Callee::Member(t.clone(), m)),
                    _ => Some(Callee::Guess(m)),
                }
            }
            _ => None,
        }
    }

    fn walk(&mut self, n: Ts, loops: u32) {
        match n.kind() {
            "function_item" => {
                let Some(name) = n.child_by_field_name("name").map(|x| self.text(x)) else {
                    return;
                };
                let kind = if self.impl_type.is_some() {
                    "method"
                } else {
                    "function"
                };
                let body = n.child_by_field_name("body");
                let (id, qual) = self.define(n, kind, name, body);
                self.scopes.push(Scope { qual, id });
                if let Some(b) = body {
                    self.walk(b, 0);
                }
                self.scopes.pop();
                return;
            }
            "struct_item" | "enum_item" | "union_item" | "trait_item" => {
                let Some(name) = n.child_by_field_name("name").map(|x| self.text(x)) else {
                    return;
                };
                let (id, qual) = self.define(n, "class", name, None);
                if n.kind() == "trait_item"
                    && let Some(b) = n.child_by_field_name("body")
                {
                    // a trait's provided methods are its own
                    let outer = self.impl_type.replace(name.into());
                    self.scopes.push(Scope { qual, id });
                    self.walk(b, 0);
                    self.scopes.pop();
                    self.impl_type = outer;
                }
                return;
            }
            "impl_item" => {
                let ty = n
                    .child_by_field_name("type")
                    .and_then(|t| type_name(t, self.src));
                let Some(ty) = ty else { return };
                if let Some(tr) = n.child_by_field_name("trait") {
                    let tr = match tr.kind() {
                        "scoped_type_identifier" => self.callee_of_type(tr),
                        _ => type_name(tr, self.src).map(Callee::Name),
                    };
                    if let Some(tr) = tr {
                        self.out.impls.push((Callee::Name(ty.clone()), tr, line(n)));
                    }
                }
                // methods are named after their type: `Type.method`, wherever the impl sits
                let outer = self.impl_type.replace(ty.clone());
                let scopes = std::mem::take(&mut self.scopes);
                // under the type when it is declared above in this file, else under the file
                let tid = format!("{}#{ty}", self.file);
                let id = if self.ids.contains_key(&tid) {
                    tid
                } else {
                    self.file.into()
                };
                self.scopes.push(Scope { qual: ty, id });
                if let Some(b) = n.child_by_field_name("body") {
                    self.walk(b, 0);
                }
                self.scopes = scopes;
                self.impl_type = outer;
                return;
            }
            "mod_item" => {
                let Some(name) = n
                    .child_by_field_name("name")
                    .map(|x| self.text(x).to_string())
                else {
                    return;
                };
                match n.child_by_field_name("body") {
                    None => self.out.mods.push((name, line(n))),
                    Some(b) => {
                        // an inline module: its items are qualified by it
                        let parent = self.scopes.last();
                        let qual = parent.map_or(name.clone(), |s| format!("{}.{name}", s.qual));
                        let id = parent.map_or_else(|| self.file.to_string(), |s| s.id.clone());
                        self.scopes.push(Scope { qual, id });
                        self.inline.push(name);
                        self.walk(b, loops);
                        self.inline.pop();
                        self.scopes.pop();
                    }
                }
                return;
            }
            "use_declaration" => {
                if let Some(a) = n.child_by_field_name("argument") {
                    self.uses(a, &vec![], line(n));
                }
                return;
            }
            "call_expression" => {
                if let Some(callee) = n
                    .child_by_field_name("function")
                    .and_then(|f| self.callee(f))
                {
                    self.out.calls.push(Ref {
                        from: String::new(),
                        callee,
                        line: line(n),
                        in_loop: loops > 0,
                    });
                }
            }
            "for_expression" | "while_expression" | "loop_expression" => {
                let mut c = n.walk();
                let kids: Vec<Ts> = n.named_children(&mut c).collect();
                for k in kids {
                    self.walk(k, loops + u32::from(k.kind() == "block"));
                }
                return;
            }
            _ => {}
        }
        let mut c = n.walk();
        let kids: Vec<Ts> = n.named_children(&mut c).collect();
        for k in kids {
            self.walk(k, loops);
        }
    }

    fn callee_of_type(&self, t: Ts) -> Option<Callee> {
        let mut p = vec![];
        let mut s = Some(t);
        while let Some(x) = s {
            match x.kind() {
                "scoped_type_identifier" | "scoped_identifier" => {
                    p.push(self.text(x.child_by_field_name("name")?).to_string());
                    s = x.child_by_field_name("path");
                }
                _ => {
                    p.push(self.text(x).to_string());
                    s = None;
                }
            }
        }
        p.reverse();
        Some(Callee::Path(p))
    }
}

/// Parses and extracts the file at `path`.
pub fn extract(path: &str, source: &str, package: Option<&str>) -> Extracted {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("the Rust grammar loads");
    let Some(tree) = parser.parse(source, None) else {
        return Extracted {
            parse_error: Some("could not parse".into()),
            ..Extracted::default()
        };
    };
    let root = tree.root_node();
    let lines = (source.strip_suffix('\n').unwrap_or(source).lines().count() as u32).max(1);
    let mut x = X {
        src: source,
        file: path,
        package,
        out: Extracted::default(),
        ids: HashMap::new(),
        scopes: vec![],
        impl_type: None,
        inline: vec![],
    };
    x.out.nodes.push(Node {
        id: path.into(),
        kind: "module".into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        file: path.into(),
        range: Range {
            start_line: 1,
            end_line: lines,
        },
        parent: None,
        package: package.map(str::to_string),
        exported: false,
        hash: sha1_bytes(source.as_bytes()),
        metrics: Metrics {
            loc: lines,
            complexity: 1,
        },
        intents: vec![],
    });
    x.walk(root, 0);
    let mut out = x.out;
    out.parse_error = root.has_error().then(|| {
        let mut at = root;
        loop {
            let mut c = at.walk();
            let next = at
                .children(&mut c)
                .find(|k| k.has_error() || k.is_missing());
            let Some(k) = next else { break };
            at = k;
            if k.is_error() || k.is_missing() {
                break;
            }
        }
        format!("syntax error at line {}", line(at))
    });
    for r in &mut out.calls {
        r.from = innermost_at(&out.nodes, r.line).to_string();
    }
    out
}
