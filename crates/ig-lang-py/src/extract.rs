//! Pass 1: one Python file's nodes (module, classes, functions, methods, tests), its imports, and
//! the calls and base classes still to resolve against the other files.

use ig_core::model::{Metrics, Node, Range, innermost_at, intents_in, normalized_hash, sha1_bytes};
use std::collections::HashMap;
use tree_sitter::{Node as Ts, Parser};

/// `import a.b` (`name` None, `local` "a"), `import a.b as c` (`local` "c"), or
/// `from m import n as k` (`name` "n", `local` "k"; "*" for both on a star import). `module` keeps
/// its leading dots when relative.
#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
    pub name: Option<String>,
    pub local: String,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub enum Callee {
    /// `f()`: a name of this file or one it imports, else matched by name.
    Name(String),
    /// `a.b.f()`, the dotted names before the last one, and the last.
    Path(Vec<String>, String),
    /// `self.m()` / `cls.m()` in class `class` (a node id).
    Member(String, String),
    /// Anything else (`x().m()`, `"".join()`): matched by name.
    Guess(String),
}

#[derive(Debug, Clone)]
pub struct Ref {
    pub from: String,
    /// "calls" or "extends".
    pub kind: &'static str,
    pub callee: Callee,
    pub line: u32,
    pub in_loop: bool,
}

#[derive(Debug, Default)]
pub struct Extracted {
    pub nodes: Vec<Node>,
    pub parse_error: Option<String>,
    pub imports: Vec<Import>,
    pub refs: Vec<Ref>,
}

struct Scope {
    qual: String,
    id: String,
    class: bool,
}

struct X<'s> {
    src: &'s str,
    file: &'s str,
    package: Option<&'s str>,
    is_test: bool,
    nodes: Vec<Node>,
    ids: HashMap<String, u32>,
    scopes: Vec<Scope>,
    imports: Vec<Import>,
    /// Refs with `from` still empty when the innermost node by line decides it.
    refs: Vec<Ref>,
}

fn line(n: Ts) -> u32 {
    n.start_position().row as u32 + 1
}

/// Decision points under `n`, not counting nested functions and classes.
fn decisions(n: Ts) -> u32 {
    let mut c = n.walk();
    n.named_children(&mut c)
        .map(|k| match k.kind() {
            "function_definition" | "class_definition" => 0,
            "if_statement"
            | "elif_clause"
            | "for_statement"
            | "while_statement"
            | "except_clause"
            | "boolean_operator"
            | "conditional_expression"
            | "case_clause"
            | "for_in_clause"
            | "if_clause" => 1 + decisions(k),
            _ => decisions(k),
        })
        .sum()
}

impl<'s> X<'s> {
    fn text(&self, n: Ts) -> &'s str {
        &self.src[n.byte_range()]
    }

    /// The comments right above `anchor` and the docstring of `body`.
    fn intents(&self, anchor: Ts, body: Option<Ts>) -> Vec<String> {
        let mut out = vec![];
        let mut p = anchor.prev_sibling();
        while let Some(c) = p.filter(|c| c.kind() == "comment") {
            intents_in(self.text(c), &mut out);
            p = c.prev_sibling();
        }
        let doc = body
            .and_then(|b| b.named_child(0))
            .filter(|s| s.kind() == "expression_statement");
        if let Some(s) = doc
            .and_then(|s| s.named_child(0))
            .filter(|s| s.kind() == "string")
        {
            intents_in(self.text(s), &mut out);
        }
        out
    }

    fn define(&mut self, def: Ts, anchor: Ts, class: bool) {
        let Some(name) = def.child_by_field_name("name").map(|n| self.text(n)) else {
            return;
        };
        let parent = self.scopes.last();
        let in_class = parent.is_some_and(|s| s.class);
        let kind = if class {
            "class"
        } else if self.is_test && name.starts_with("test") && (parent.is_none() || in_class) {
            "test"
        } else if in_class {
            "method"
        } else {
            "function"
        };
        let qual = parent.map_or_else(|| name.to_string(), |s| format!("{}.{name}", s.qual));
        let base = format!("{}#{qual}", self.file);
        let n = self.ids.entry(base.clone()).or_insert(0);
        *n += 1;
        let (id, qual) = match *n {
            1 => (base, qual),
            n => (format!("{base}~{n}"), format!("{qual}~{n}")),
        };
        let body = def.child_by_field_name("body");
        let (start, end) = (line(anchor), def.end_position().row as u32 + 1);
        self.nodes.push(Node {
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
            exported: parent.is_none() && !name.starts_with('_') && kind != "test",
            hash: normalized_hash(self.text(anchor)),
            metrics: Metrics {
                loc: end - start + 1,
                complexity: 1 + body.map_or(0, decisions),
            },
            intents: self.intents(anchor, body),
        });
        if class && let Some(bases) = def.child_by_field_name("superclasses") {
            let mut c = bases.walk();
            for b in bases.named_children(&mut c) {
                if let Some(callee) = self.callee(b) {
                    self.refs.push(Ref {
                        from: id.clone(),
                        kind: "extends",
                        callee,
                        line: line(b),
                        in_loop: false,
                    });
                }
            }
        }
        self.scopes.push(Scope { qual, id, class });
        if let Some(b) = body {
            self.walk(b, 0);
        }
        self.scopes.pop();
    }

    /// What an expression in call or base position names.
    fn callee(&self, f: Ts) -> Option<Callee> {
        match f.kind() {
            "identifier" => Some(Callee::Name(self.text(f).into())),
            "attribute" => {
                let attr = self.text(f.child_by_field_name("attribute")?).to_string();
                let mut chain = vec![];
                let mut o = f.child_by_field_name("object")?;
                loop {
                    match o.kind() {
                        "identifier" => {
                            chain.push(self.text(o).to_string());
                            break;
                        }
                        "attribute" => {
                            chain.push(self.text(o.child_by_field_name("attribute")?).into());
                            o = o.child_by_field_name("object")?;
                        }
                        _ => return Some(Callee::Guess(attr)),
                    }
                }
                chain.reverse();
                let class = self.scopes.iter().rev().find(|s| s.class);
                if let ([me], Some(c)) = (&chain[..], class)
                    && (me == "self" || me == "cls")
                {
                    return Some(Callee::Member(c.id.clone(), attr));
                }
                Some(Callee::Path(chain, attr))
            }
            _ => None,
        }
    }

    fn import(&mut self, n: Ts) {
        let mut c = n.walk();
        let at = line(n);
        if n.kind() == "import_statement" {
            for name in n.children_by_field_name("name", &mut c) {
                let (module, alias) = match name.kind() {
                    "aliased_import" => (
                        name.child_by_field_name("name").map(|x| self.text(x)),
                        name.child_by_field_name("alias").map(|x| self.text(x)),
                    ),
                    _ => (Some(self.text(name)), None),
                };
                let Some(module) = module else { continue };
                let local = alias.unwrap_or_else(|| module.split('.').next().unwrap_or(module));
                self.imports.push(Import {
                    module: module.into(),
                    name: None,
                    local: local.into(),
                    line: at,
                });
            }
            return;
        }
        let Some(module) = n
            .child_by_field_name("module_name")
            .map(|m| self.text(m).to_string())
        else {
            return;
        };
        let module: String = module.split_whitespace().collect();
        let mut star = n.walk();
        if n.children(&mut star).any(|k| k.kind() == "wildcard_import") {
            self.imports.push(Import {
                module,
                name: Some("*".into()),
                local: "*".into(),
                line: at,
            });
            return;
        }
        for name in n.children_by_field_name("name", &mut c) {
            let (nm, alias) = match name.kind() {
                "aliased_import" => (
                    name.child_by_field_name("name").map(|x| self.text(x)),
                    name.child_by_field_name("alias").map(|x| self.text(x)),
                ),
                _ => (Some(self.text(name)), None),
            };
            let Some(nm) = nm else { continue };
            self.imports.push(Import {
                module: module.clone(),
                name: Some(nm.into()),
                local: alias.unwrap_or(nm).into(),
                line: at,
            });
        }
    }

    /// `loops`: loops around this point inside the current function.
    fn walk(&mut self, n: Ts, loops: u32) {
        match n.kind() {
            "decorated_definition" => {
                let mut c = n.walk();
                for d in n.named_children(&mut c).filter(|d| d.kind() == "decorator") {
                    self.walk(d, loops);
                }
                if let Some(def) = n.child_by_field_name("definition") {
                    self.define(def, n, def.kind() == "class_definition");
                }
                return;
            }
            "function_definition" | "class_definition" => {
                return self.define(n, n, n.kind() == "class_definition");
            }
            "import_statement" | "import_from_statement" => return self.import(n),
            "call" => {
                if let Some(callee) = n
                    .child_by_field_name("function")
                    .and_then(|f| self.callee(f))
                {
                    self.refs.push(Ref {
                        from: String::new(),
                        kind: "calls",
                        callee,
                        line: line(n),
                        in_loop: loops > 0,
                    });
                }
            }
            "for_statement" | "while_statement" => {
                let mut c = n.walk();
                for k in n.named_children(&mut c) {
                    let body = matches!(k.kind(), "block");
                    self.walk(k, loops + u32::from(body));
                }
                return;
            }
            "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression" => {
                let mut c = n.walk();
                for k in n.named_children(&mut c) {
                    self.walk(k, loops + 1);
                }
                return;
            }
            _ => {}
        }
        let mut c = n.walk();
        for k in n.named_children(&mut c) {
            self.walk(k, loops);
        }
    }
}

/// Parses and extracts the file at `path`.
pub fn extract(path: &str, source: &str, package: Option<&str>, is_test: bool) -> Extracted {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("the Python grammar loads");
    let Some(tree) = parser.parse(source, None) else {
        return Extracted {
            parse_error: Some("could not parse".into()),
            ..Extracted::default()
        };
    };
    let root = tree.root_node();
    let lines = (source.strip_suffix('\n').unwrap_or(source).lines().count() as u32).max(1);
    let module = Node {
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
    };
    let mut x = X {
        src: source,
        file: path,
        package,
        is_test,
        nodes: vec![module],
        ids: HashMap::new(),
        scopes: vec![],
        imports: vec![],
        refs: vec![],
    };
    x.walk(root, 0);
    let parse_error = root.has_error().then(|| {
        let mut c = root.walk();
        let mut at = root;
        // the first error or missing node, depth first
        'down: loop {
            for k in at.children(&mut c) {
                if k.is_error() || k.is_missing() {
                    break 'down format!("syntax error at line {}", line(k));
                }
                if k.has_error() {
                    at = k;
                    continue 'down;
                }
            }
            break format!("syntax error at line {}", line(at));
        }
    });
    for r in x.refs.iter_mut().filter(|r| r.from.is_empty()) {
        r.from = innermost_at(&x.nodes, r.line).to_string();
    }
    Extracted {
        nodes: x.nodes,
        parse_error,
        imports: x.imports,
        refs: x.refs,
    }
}
