use ig_core::model::{Metrics, Node, Range, innermost_at, intents_in, normalized_hash, sha1_bytes};
use std::collections::HashMap;
use tree_sitter::{Node as Ts, Parser};

#[derive(Debug, Clone, PartialEq)]
pub enum Local {
    Named(String),
    Dot,
    Blank,
    Default,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub path: String,
    pub local: Local,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub enum Callee {
    Name(String),
    Qualified(String, String),
    Member(String, String),
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
    pub package: Option<String>,
    pub nodes: Vec<Node>,
    pub parse_error: Option<String>,
    pub imports: Vec<Import>,
    pub calls: Vec<Ref>,
    pub qualified: Vec<(String, String, u32)>,
}

struct X<'s> {
    src: &'s str,
    file: &'s str,
    package: Option<&'s str>,
    is_test: bool,
    out: Extracted,
    ids: HashMap<String, u32>,
    receiver: Option<(String, String)>,
}

fn line(n: Ts) -> u32 {
    n.start_position().row as u32 + 1
}

fn decisions(n: Ts, src: &str) -> u32 {
    let mut c = n.walk();
    n.named_children(&mut c)
        .map(|k| match k.kind() {
            "func_literal" => 0,
            "if_statement" | "for_statement" | "expression_case" | "type_case"
            | "communication_case" => 1 + decisions(k, src),
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

fn unquote(s: &str) -> &str {
    s.trim_matches(|c| c == '"' || c == '`')
}

fn base_type<'t>(t: Ts<'t>) -> Option<Ts<'t>> {
    match t.kind() {
        "type_identifier" => Some(t),
        "pointer_type" | "parenthesized_type" => base_type(t.named_child(0)?),
        "generic_type" => base_type(t.child_by_field_name("type")?),
        _ => None,
    }
}

fn is_test_name(name: &str) -> bool {
    ["Test", "Benchmark", "Example", "Fuzz"].iter().any(|p| {
        name.strip_prefix(p)
            .is_some_and(|r| !r.starts_with(|c: char| c.is_lowercase()))
    })
}

impl<'s> X<'s> {
    fn text(&self, n: Ts) -> &'s str {
        &self.src[n.byte_range()]
    }

    fn intents(&self, n: Ts) -> Vec<String> {
        let mut out = vec![];
        let mut p = n.prev_sibling();
        while let Some(c) = p.filter(|c| c.kind() == "comment") {
            intents_in(self.text(c), &mut out);
            p = c.prev_sibling();
        }
        out
    }

    fn node(
        &mut self,
        def: Ts,
        anchor: Ts,
        name: &str,
        qual: String,
        kind: &str,
        parent: String,
    ) -> String {
        let base = format!("{}#{qual}", self.file);
        let n = self.ids.entry(base.clone()).or_insert(0);
        *n += 1;
        let id = match *n {
            1 => base,
            n => format!("{base}~{n}"),
        };
        let body = def.child_by_field_name("body");
        let (start, end) = (line(anchor), def.end_position().row as u32 + 1);
        self.out.nodes.push(Node {
            id: id.clone(),
            kind: kind.into(),
            name: name.into(),
            file: self.file.into(),
            range: Range {
                start_line: start,
                end_line: end,
            },
            parent: Some(parent),
            package: self.package.map(str::to_string),
            exported: kind != "test" && name.starts_with(|c: char| c.is_uppercase()),
            hash: normalized_hash(self.text(def)),
            metrics: Metrics {
                loc: end - start + 1,
                complexity: 1 + body.map_or(0, |b| decisions(b, self.src)),
            },
            intents: self.intents(anchor),
        });
        id
    }

    fn function(&mut self, n: Ts) {
        let Some(name) = n.child_by_field_name("name").map(|x| self.text(x)) else {
            return;
        };
        let kind = if self.is_test && is_test_name(name) {
            "test"
        } else {
            "function"
        };
        self.node(n, n, name, name.into(), kind, self.file.into());
        self.body(n);
    }

    fn method(&mut self, n: Ts) {
        let Some(name) = n.child_by_field_name("name").map(|x| self.text(x)) else {
            return;
        };
        let recv = n
            .child_by_field_name("receiver")
            .and_then(|r| r.named_child(0))
            .filter(|p| p.kind() == "parameter_declaration");
        let ty = recv
            .and_then(|p| p.child_by_field_name("type"))
            .and_then(base_type)
            .map(|t| self.text(t).to_string());
        let Some(ty) = ty else {
            self.node(n, n, name, name.into(), "method", self.file.into());
            return self.body(n);
        };
        let type_id = format!("{}#{ty}", self.file);
        let parent = match self.out.nodes.iter().any(|x| x.id == type_id) {
            true => type_id,
            false => self.file.into(),
        };
        self.node(n, n, name, format!("{ty}.{name}"), "method", parent);
        let var = recv
            .and_then(|p| p.child_by_field_name("name"))
            .map(|v| self.text(v).to_string());
        self.receiver = var.map(|v| (v, ty));
        self.body(n);
        self.receiver = None;
    }

    fn types(&mut self, decl: Ts) {
        let mut c = decl.walk();
        let specs: Vec<Ts> = decl
            .named_children(&mut c)
            .filter(|s| matches!(s.kind(), "type_spec" | "type_alias"))
            .collect();
        let single = specs.len() == 1;
        for s in specs {
            let Some(name) = s.child_by_field_name("name").map(|x| self.text(x)) else {
                continue;
            };
            let anchor = if single { decl } else { s };
            self.node(s, anchor, name, name.into(), "class", self.file.into());
            self.walk(s, 0);
        }
    }

    fn body(&mut self, n: Ts) {
        let mut c = n.walk();
        let kids: Vec<Ts> = n.named_children(&mut c).collect();
        for k in kids {
            self.walk(k, 0);
        }
    }

    fn imports(&mut self, decl: Ts) {
        let mut stack = vec![decl];
        while let Some(n) = stack.pop() {
            if n.kind() != "import_spec" {
                let mut c = n.walk();
                let kids: Vec<Ts> = n.named_children(&mut c).collect();
                stack.extend(kids.into_iter().rev());
                continue;
            }
            let Some(path) = n.child_by_field_name("path") else {
                continue;
            };
            let local = match n.child_by_field_name("name") {
                None => Local::Default,
                Some(a) if a.kind() == "dot" => Local::Dot,
                Some(a) if a.kind() == "blank_identifier" => Local::Blank,
                Some(a) => Local::Named(self.text(a).into()),
            };
            self.out.imports.push(Import {
                path: unquote(self.text(path)).into(),
                local,
                line: line(n),
            });
        }
    }

    fn callee(&self, f: Ts) -> Option<Callee> {
        match f.kind() {
            "identifier" => Some(Callee::Name(self.text(f).into())),
            "parenthesized_expression" => self.callee(f.named_child(0)?),
            "generic_type" | "index_expression" => {
                let inner = f
                    .child_by_field_name("type")
                    .or_else(|| f.child_by_field_name("operand"))?;
                self.callee(inner)
            }
            "selector_expression" => {
                let field = self.text(f.child_by_field_name("field")?).to_string();
                let op = f.child_by_field_name("operand")?;
                if op.kind() != "identifier" {
                    return Some(Callee::Guess(field));
                }
                let op = self.text(op);
                match &self.receiver {
                    Some((v, ty)) if v == op => Some(Callee::Member(ty.clone(), field)),
                    _ => Some(Callee::Qualified(op.into(), field)),
                }
            }
            _ => None,
        }
    }

    fn walk(&mut self, n: Ts, loops: u32) {
        match n.kind() {
            "package_clause" => {
                self.out.package = n.named_child(0).map(|p| self.text(p).to_string());
                return;
            }
            "import_declaration" => return self.imports(n),
            "function_declaration" => return self.function(n),
            "method_declaration" => return self.method(n),
            "type_declaration" => return self.types(n),
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
            "selector_expression" => {
                let op = n
                    .child_by_field_name("operand")
                    .filter(|o| o.kind() == "identifier");
                if let (Some(op), Some(field)) = (op, n.child_by_field_name("field")) {
                    let q = (self.text(op).into(), self.text(field).into(), line(n));
                    self.out.qualified.push(q);
                }
            }
            "qualified_type" => {
                let (p, t) = (
                    n.child_by_field_name("package"),
                    n.child_by_field_name("name"),
                );
                if let (Some(p), Some(t)) = (p, t) {
                    let q = (self.text(p).into(), self.text(t).into(), line(n));
                    self.out.qualified.push(q);
                }
            }
            "for_statement" => {
                let mut c = n.walk();
                for k in n.named_children(&mut c) {
                    let body = k.kind() == "block";
                    self.walk(k, loops + u32::from(body));
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

pub fn extract(path: &str, source: &str, package: Option<&str>, is_test: bool) -> Extracted {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("the Go grammar loads");
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
        out: Extracted {
            nodes: vec![module],
            ..Extracted::default()
        },
        ids: HashMap::new(),
        receiver: None,
    };
    x.walk(root, 0);
    let mut out = x.out;
    out.parse_error = root.has_error().then(|| {
        let mut c = root.walk();
        let mut at = root;
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
    for r in out.calls.iter_mut() {
        r.from = innermost_at(&out.nodes, r.line).to_string();
    }
    out
}
