//! The code level, as deep as the code goes: inside each component its folders and files,
//! their classes and functions, and inside a class its methods. A box holds at most 12: past
//! that its items are grouped by subfolder or file, else by name prefix, else alphabetically,
//! and every group opens onto the next level, so nothing is out of reach. A UI component (a Vue
//! file, a React function component) is one box opening onto its parts (inputs, state, uses,
//! template) and its functions. Shown only; never stored in the map file.

use super::facts::{detect, other, slot};
use super::file::{MapBox, MapFile, fresh};
use super::{BoxKind, FactBox, Guess, Level, MapArrow, Pattern, arrows};
use ig_core::model::{Anatomy, Graph, Node, Part, Range};
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_CODE: usize = 12;

/// A map box as the facts `arrows` lifts edges over.
pub(crate) fn as_fact(b: &MapBox) -> FactBox {
    FactBox {
        id: b.id.clone(),
        parent: b.parent.clone(),
        level: b.level,
        made_of: b.made_of.clone(),
        package: b.package.clone(),
        files: b.files,
        nodes: b.nodes.clone(),
        guess: Guess {
            kind: b.kind.value,
            title: b.title.value.clone(),
            pattern: b.pattern.as_ref().map(|p| p.value.clone()),
            slot: b.slot.as_ref().map(|s| s.value.clone()),
            logo: b.logo.as_ref().map(|l| l.value.clone()),
        },
    }
}

/// The code boxes of every component, nested (see the module doc); a function, class or method
/// box's id is its graph node id, a file's its module id. Arrows between them are lifted as in
/// `arrows`, keeping those inside a component. `ui` are the components the code declares.
pub fn code_boxes(m: &MapFile, g: &Graph, ui: &[Anatomy]) -> Code {
    let mut members: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
    for n in &g.nodes {
        if let Some(p) = &n.parent {
            members.entry(p).or_default().push(n);
        }
    }
    let anatomy = anatomies(g, ui);
    let mut code = Nest {
        members,
        anatomy: &anatomy,
        out: vec![],
        code: vec![],
        parts: vec![],
        patterns: vec![],
        taken: m.boxes.iter().map(|b| b.id.clone()).collect(),
    };
    for c in m
        .boxes
        .iter()
        .filter(|b| b.level == Level::Component && !b.stale)
    {
        let modules: HashSet<&str> = c.nodes.iter().map(String::as_str).collect();
        // a single-file component stands for its whole file
        let tops: Vec<&Node> = g
            .nodes
            .iter()
            .filter(|n| {
                let top = matches!(n.kind.as_str(), "function" | "class" | "task" | "document")
                    && n.parent
                        .as_deref()
                        .is_some_and(|p| modules.contains(p) && !anatomy.contains_key(p));
                top || (n.is_module()
                    && modules.contains(n.id.as_str())
                    && anatomy.contains_key(n.id.as_str()))
            })
            .collect();
        code.nest(c, &c.id, tops);
    }
    let (code, spans, parts, patterns) = (code.out, code.code, code.parts, code.patterns);
    let inside: HashSet<&str> = m
        .boxes
        .iter()
        .filter(|b| b.level == Level::Component)
        .map(|b| b.id.as_str())
        .chain(code.iter().map(|f| f.id.as_str()))
        .collect();
    let all: Vec<FactBox> = m
        .boxes
        .iter()
        .map(as_fact)
        .chain(code.iter().cloned())
        .collect();
    let lifted = arrows(&all, g)
        .into_iter()
        .filter(|a| inside.contains(a.parent.as_str()))
        .collect();
    let boxes = code
        .iter()
        .map(|f| MapBox {
            needs_reading: false,
            ..fresh(f)
        })
        .collect();
    Code {
        boxes,
        arrows: lifted,
        spans,
        parts,
        patterns,
    }
}

/// The code level: its boxes and arrows, the lines of code some boxes show and the part lists
/// of a component's parts boxes.
pub struct Code {
    pub boxes: Vec<MapBox>,
    pub arrows: Vec<MapArrow>,
    /// (box id, file, its lines, complexity).
    pub spans: Vec<(String, String, Range, u32)>,
    pub parts: Vec<(String, Vec<Part>)>,
    /// (box id, the pattern its folders show), for boxes with none of their own.
    pub patterns: Vec<(String, Pattern)>,
}

/// Each component's node: a single-file component's module, a React component's function (by
/// name in its file, the nearest to its first line).
fn anatomies<'u>(g: &Graph, ui: &'u [Anatomy]) -> HashMap<String, &'u Anatomy> {
    let mut out = HashMap::new();
    for a in ui {
        let n = if a.framework == "react" {
            g.nodes
                .iter()
                .filter(|n| n.file == a.file && n.kind == "function")
                .filter(|n| n.name == a.name || n.name == "default")
                .min_by_key(|n| n.range.start_line.abs_diff(a.range.start_line))
        } else {
            g.nodes.iter().find(|n| n.is_module() && n.file == a.file)
        };
        if let Some(n) = n {
            out.insert(n.id.clone(), a);
        }
    }
    out
}

/// One group of items: (id, title, kind, whole modules it holds, items).
type Group<'g> = (String, String, BoxKind, Vec<&'g str>, Vec<&'g Node>);

struct Nest<'g, 'u> {
    /// Graph nodes by parent id.
    members: BTreeMap<&'g str, Vec<&'g Node>>,
    /// The component each node is, by node id.
    anatomy: &'u HashMap<String, &'u Anatomy>,
    out: Vec<FactBox>,
    /// See `Code`.
    code: Vec<(String, String, Range, u32)>,
    parts: Vec<(String, Vec<Part>)>,
    /// See `Code`.
    patterns: Vec<(String, Pattern)>,
    /// Box ids in use.
    taken: HashSet<String>,
}

impl<'g> Nest<'g, '_> {
    /// `items` as the children of `parent`: one box each when they fit, else their groups.
    fn nest(&mut self, c: &MapBox, parent: &str, items: Vec<&'g Node>) {
        if items.len() <= MAX_CODE {
            for n in items {
                if let Some(a) = self.anatomy.get(n.id.as_str()).copied() {
                    self.component(c, parent, n, a);
                    continue;
                }
                let kind = match n.kind.as_str() {
                    "class" => BoxKind::Class,
                    "task" => BoxKind::Task,
                    "document" => BoxKind::File,
                    "method" => BoxKind::Method,
                    _ => BoxKind::Function,
                };
                let id = self.push(c, parent, (n.id.clone(), n.name.clone(), kind), &[], &[n]);
                self.code
                    .push((id.clone(), n.file.clone(), n.range, n.metrics.complexity));
                let kids: Vec<&Node> = self
                    .members
                    .get(n.id.as_str())
                    .into_iter()
                    .flatten()
                    .filter(|m| matches!(m.kind.as_str(), "function" | "class" | "method"))
                    .copied()
                    .collect();
                self.nest(c, &id, kids);
            }
            return;
        }
        // folders laid out as an onion, hexagon or bands show it at any depth, as a package does
        let groups = groups(parent, items);
        let folders = (groups.iter())
            .filter(|g| g.2 == BoxKind::Folder)
            .map(|g| g.1.trim_end_matches('/').to_string())
            .collect();
        let pattern = Some(detect(&folders)).filter(|p| *p != Pattern::None);
        if let Some(p) = &pattern {
            self.patterns.push((parent.into(), p.clone()));
        }
        for (id, title, kind, modules, group) in groups {
            let s = pattern.as_ref().and_then(|p| {
                let named = (kind == BoxKind::Folder)
                    .then(|| slot(p, title.trim_end_matches('/')))
                    .flatten();
                named.or(other(p))
            });
            let id = self.push(c, parent, (id, title, kind), &modules, &group);
            if let Some(f) = self.out.last_mut() {
                f.guess.slot = s.map(String::from);
            }
            self.nest(c, &id, group);
        }
    }

    /// Component `a` (node `n`) under `parent`: its part lists, then its functions (those not
    /// already listed as state or drawn), in a group of their own when they would not fit beside
    /// them.
    fn component(&mut self, c: &MapBox, parent: &str, n: &'g Node, a: &Anatomy) {
        let modules: Vec<&str> = if n.is_module() {
            vec![n.id.as_str()]
        } else {
            vec![]
        };
        let items: Vec<&Node> = if n.is_module() { vec![] } else { vec![n] };
        let title = (a.name.clone(), BoxKind::Component);
        let id = self.push(
            c,
            parent,
            (n.id.clone(), title.0, title.1),
            &modules,
            &items,
        );
        if let Some(f) = self.out.last_mut() {
            f.guess.logo = match a.framework.as_str() {
                "vue" => Some("vue".into()),
                "react" => Some("react".into()),
                _ => None,
            };
            if n.is_module() {
                f.made_of = vec![n.file.clone()];
                f.files = 1;
            }
        }
        // a single-file component's code is its template's and its functions': not again whole
        if !n.is_module() {
            self.code
                .push((id.clone(), a.file.clone(), a.range, n.metrics.complexity));
        }
        let template = if a.template.is_some() {
            "Template"
        } else {
            "Renders"
        };
        let lists = [
            ("inputs", "Inputs", &a.inputs),
            ("state", "State", &a.state),
            ("uses", "Uses", &a.uses),
            ("renders", template, &a.renders),
        ];
        let mut shown = 0;
        for (key, name, parts) in lists {
            if parts.is_empty() && !(key == "renders" && a.template.is_some()) {
                continue;
            }
            let pid = self.push(
                c,
                &id,
                (format!("{id}/{key}"), name.into(), BoxKind::Parts),
                &[],
                &[],
            );
            if let Some(f) = self.out.last_mut() {
                f.made_of = vec![a.file.clone()];
            }
            self.parts.push((pid.clone(), parts.clone()));
            if let (Some(t), "renders") = (a.template, key) {
                self.code.push((pid, a.file.clone(), t, 0));
            }
            shown += 1;
        }
        // a computed value is state; `const LazyRow = defineAsyncComponent(…)` a component drawn
        let listed: HashSet<&str> = (a.state.iter().chain(&a.renders))
            .map(|p| p.name.as_str())
            .collect();
        let fns: Vec<&Node> = (self.members.get(n.id.as_str()).into_iter().flatten())
            .filter(|m| matches!(m.kind.as_str(), "function" | "class" | "method"))
            .filter(|m| !listed.contains(m.name.as_str()))
            .copied()
            .collect();
        if fns.len() + shown <= MAX_CODE {
            self.nest(c, &id, fns);
        } else {
            let at = (
                format!("{id}/functions"),
                "Functions".into(),
                BoxKind::Group,
            );
            let fid = self.push(c, &id, at, &[], &fns);
            self.nest(c, &fid, fns);
        }
    }

    /// Adds the box of `items` (and every node under them, and `modules`) under `parent`;
    /// returns its id, made unique.
    fn push(
        &mut self,
        c: &MapBox,
        parent: &str,
        (id, title, kind): (String, String, BoxKind),
        modules: &[&str],
        items: &[&'g Node],
    ) -> String {
        let mut unique = id.clone();
        for n in 2.. {
            if self.taken.insert(unique.clone()) {
                break;
            }
            unique = format!("{id}~{n}");
        }
        let mut nodes: Vec<String> = modules.iter().map(|m| m.to_string()).collect();
        let mut stack: Vec<&Node> = items.to_vec();
        let mut files: Vec<String> = vec![];
        while let Some(n) = stack.pop() {
            nodes.push(n.id.clone());
            files.push(n.file.clone());
            stack.extend(self.members.get(n.id.as_str()).into_iter().flatten());
        }
        files.sort();
        files.dedup();
        self.out.push(FactBox {
            id: unique.clone(),
            parent: Some(parent.into()),
            level: Level::Code,
            files: files.len() as u32,
            made_of: files,
            package: c.package.clone(),
            nodes,
            guess: Guess {
                kind,
                title,
                pattern: None,
                slot: None,
                logo: None,
            },
        });
        unique
    }
}

/// More than 12 `items` of `parent` split into at most 12 groups, each smaller than `items`:
/// by the subfolder or file they sit in below their common folder, else by name prefix, else
/// alphabetically. Past 12 groups, neighbouring groups share an alphabetical range box.
fn groups<'g>(parent: &str, mut items: Vec<&'g Node>) -> Vec<Group<'g>> {
    items.sort_by(|a, b| (&a.file, &a.name, &a.id).cmp(&(&b.file, &b.name, &b.id)));
    let dir = |f: &'g str| f.rsplit_once('/').map_or("", |d| d.0);
    let common = items
        .iter()
        .map(|n| dir(&n.file).split('/').collect::<Vec<_>>())
        .reduce(|a, b| {
            a.into_iter()
                .zip(b)
                .take_while(|(x, y)| x == y)
                .map(|p| p.0)
                .collect()
        })
        .unwrap_or_default()
        .join("/");
    let common = common.as_str();
    let mut by_place: BTreeMap<(bool, String), Vec<&Node>> = BTreeMap::new();
    for n in &items {
        let rest = dir(&n.file)
            .strip_prefix(common)
            .unwrap_or("")
            .trim_start_matches('/');
        let key = match rest.split('/').next().filter(|s| !s.is_empty()) {
            Some(sub) if common.is_empty() => (true, format!("{sub}/")),
            Some(sub) => (true, format!("{common}/{sub}/")),
            None => (false, n.file.clone()),
        };
        by_place.entry(key).or_default().push(n);
    }
    let mut out: Vec<Group> = if by_place.len() > 1 {
        by_place
            .into_iter()
            .map(|((folder, path), ns)| {
                let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
                // the modules themselves too, so imports between files are drawn
                let mut ms: Vec<&str> = ns.iter().filter_map(|n| n.parent.as_deref()).collect();
                ms.sort();
                ms.dedup();
                if folder {
                    (path.clone(), format!("{name}/"), BoxKind::Folder, ms, ns)
                } else {
                    (path.clone(), name.to_string(), BoxKind::File, ms, ns)
                }
            })
            .collect()
    } else {
        let mut by_prefix: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
        for n in &items {
            by_prefix.entry(prefix(&n.name)).or_default().push(n);
        }
        if by_prefix.len() > 1 {
            by_prefix
                .into_iter()
                .map(|(p, ns)| {
                    (
                        format!("{parent}/{p}"),
                        format!("{p}…"),
                        BoxKind::Group,
                        vec![],
                        ns,
                    )
                })
                .collect()
        } else {
            items.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
            let per = items.len().div_ceil(MAX_CODE);
            return items
                .chunks(per)
                .map(|ns| {
                    range(
                        parent,
                        vec![],
                        ns.to_vec(),
                        &ns[0].name,
                        &ns[ns.len() - 1].name,
                    )
                })
                .collect();
        }
    };
    if out.len() > MAX_CODE {
        let per = out.len().div_ceil(MAX_CODE);
        let mut chunks: Vec<Vec<Group>> = vec![];
        for g in out {
            match chunks.last_mut() {
                Some(c) if c.len() < per => c.push(g),
                _ => chunks.push(vec![g]),
            }
        }
        out = chunks
            .into_iter()
            .map(|mut gs| {
                if gs.len() == 1 {
                    return gs.pop().unwrap();
                }
                let (a, b) = (gs[0].1.clone(), gs[gs.len() - 1].1.clone());
                let ms = gs.iter().flat_map(|g| g.3.clone()).collect();
                let ns = gs.into_iter().flat_map(|g| g.4).collect();
                range(parent, ms, ns, &a, &b)
            })
            .collect();
    }
    out
}

/// A box for the names `a` to `b`.
fn range<'g>(parent: &str, ms: Vec<&'g str>, ns: Vec<&'g Node>, a: &str, b: &str) -> Group<'g> {
    let t = |s: &str| s.trim_end_matches(['/', '…']).to_string();
    let (a, b) = (t(a), t(b));
    (
        format!("{parent}/{a}..{b}"),
        format!("{a} – {b}"),
        BoxKind::Group,
        ms,
        ns,
    )
}

/// A name's first word: `getUser` → `get`, `load_all` → `load`.
fn prefix(name: &str) -> &str {
    name.char_indices()
        .skip(1)
        .find(|(_, ch)| ch.is_uppercase() || *ch == '_' || ch.is_ascii_digit())
        .map_or(name, |(i, _)| &name[..i])
}

#[cfg(test)]
mod tests;
