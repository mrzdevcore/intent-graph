//! `ig/*.kdl` back into a map, strictly: any problem is an error with file and line, and
//! nothing partial is returned. Every field read is a person's and pinned.

use super::DIR;
use crate::map::file::{Field, MapBox, MapFile, PINNABLE, Source, validate_spec};
use crate::map::{ArrowKind, BoxKind, Level, MapArrow, Pattern};
use globset::GlobBuilder;
use ig_core::ports::Files;
use kdl::{KdlDocument, KdlNode};
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};

/// The map written in `ig/`; `None` when it holds no `.kdl` file.
pub fn read(files: &dyn Files) -> Result<Option<MapFile>, Vec<String>> {
    let paths = files
        .glob(&[format!("{DIR}/*.kdl")])
        .map_err(|e| vec![format!("{DIR}: {e}")])?;
    if paths.is_empty() {
        return Ok(None);
    }
    let texts = paths.into_iter().map(|p| {
        let t = files.read(&p);
        (p, t)
    });
    parse(texts.collect()).map(Some)
}

/// The map in spec files given as (path, text; none when it cannot be read).
pub fn parse(texts: Vec<(String, Option<String>)>) -> Result<MapFile, Vec<String>> {
    let mut r = Reader::default();
    let mut docs = vec![];
    for (p, text) in texts {
        let Some(text) = text else {
            r.errs.push(format!("{p}: cannot be read"));
            continue;
        };
        match text.parse::<KdlDocument>() {
            Ok(doc) => docs.push((p, text, doc)),
            Err(e) => {
                for d in &e.diagnostics {
                    // The span runs from the node's start to where parsing gave up.
                    let msg = d.message.clone().unwrap_or_else(|| "syntax error".into());
                    r.errs.push(format!(
                        "{p}:{}: {msg}",
                        line(&text, d.span.offset() + d.span.len())
                    ));
                }
            }
        }
    }
    // what follows a syntax error is noise
    if !r.errs.is_empty() {
        return Err(r.errs);
    }
    let systems: Vec<(&str, usize, &str)> = docs
        .iter()
        .flat_map(|(p, t, d)| d.nodes().iter().map(move |n| (p, t, n)))
        .filter(|(_, _, n)| n.name().value() == "system")
        .map(|(p, t, n)| {
            let id = n.entries().first().and_then(|e| e.value().as_string());
            (
                p.as_str(),
                line(t, n.span().offset()),
                id.unwrap_or_default(),
            )
        })
        .collect();
    match systems.get(1).or(systems.first()) {
        None => r.errs.push(format!("{DIR}/system.kdl:1: no system node")),
        Some((p, l, _)) if systems.len() > 1 => r.errs.push(format!(
            "{p}:{l}: a second system node, expected exactly one"
        )),
        _ => {}
    }
    let sys = systems.first().map(|s| s.2).unwrap_or_default().to_string();
    for (p, text, doc) in &docs {
        let top = doc.nodes();
        if top.len() != 1 && !p.ends_with("/system.kdl") {
            r.errs.push(format!(
                "{p}:1: expected exactly one top-level node, found {}",
                top.len()
            ));
        }
        let at = Src { path: p, text };
        for n in top {
            // Outside system.kdl a top-level node sits directly under the system.
            let nested = n.name().value() != "system" && !p.ends_with("/system.kdl");
            r.node(at, n, nested.then_some(sys.as_str()), &sys);
        }
    }
    r.finish()
}

#[derive(Clone, Copy)]
struct Src<'a> {
    path: &'a str,
    text: &'a str,
}

#[derive(Default)]
struct Reader {
    boxes: Vec<MapBox>,
    /// An arrow and where it was written.
    arrows: Vec<(MapArrow, String)>,
    errs: Vec<String>,
}

fn line(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

fn named<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.into())).ok()
}

fn strings(n: &KdlNode) -> Option<Vec<String>> {
    n.entries()
        .iter()
        .filter(|e| e.name().is_none())
        .map(|e| e.value().as_string().map(String::from))
        .collect()
}

impl Reader {
    fn err(&mut self, at: Src, n: &KdlNode, what: String) {
        self.errs.push(format!(
            "{}:{}: {what}",
            at.path,
            line(at.text, n.span().offset())
        ));
    }

    fn node(&mut self, at: Src, n: &KdlNode, parent: Option<&str>, sys: &str) {
        let name = n.name().value();
        let Some(kind) = named::<BoxKind>(name) else {
            return self.err(at, n, format!("`{name}` is not a box kind"));
        };
        use BoxKind::{Class, File, Function, Method, Parts, Task};
        if [Function, Class, Method, File, Parts, Task].contains(&kind) {
            return self.err(
                at,
                n,
                format!("`{name}` is a code-level kind, not one for ig/"),
            );
        }
        if kind == BoxKind::System && parent.is_some() {
            return self.err(at, n, "a `system` node inside another box".into());
        }
        let Some(id) = n.entries().first().and_then(|e| e.value().as_string()) else {
            return self.err(at, n, format!("`{name}` needs an id string"));
        };
        let level = match parent {
            None => Level::System,
            Some(p) if p == sys => Level::Container,
            Some(_) => Level::Component,
        };
        let mut b = MapBox {
            id: id.into(),
            parent: parent.map(String::from),
            level,
            made_of: vec![],
            made_of_source: Source::Person,
            package: None,
            files: 0,
            kind: Field::person(kind),
            title: Field::person(String::new()),
            tech: None,
            description: None,
            logo: None,
            badges: None,
            pattern: None,
            slot: None,
            service_name: None,
            pinned: PINNABLE.iter().map(|s: &&str| s.to_string()).collect(),
            needs_reading: false,
            stale: false,
            nodes: vec![],
            moved_from: vec![],
            origin: Default::default(),
        };
        self.props(at, n, &mut b);
        if self.boxes.iter().any(|x| x.id == id) {
            self.err(at, n, format!("id `{id}` is used twice"));
        }
        let id = id.to_string();
        let mut body = BTreeSet::new();
        let kids = n.children().map(|c| c.nodes()).unwrap_or_default();
        for k in kids {
            let kn = k.name().value();
            if !body.insert(kn) && ["at", "badges", "pattern"].contains(&kn) {
                self.err(at, k, format!("`{kn}` is written twice"));
            }
            match (kn, strings(k)) {
                ("at", Some(v)) => {
                    for g in &v {
                        if let Err(e) = GlobBuilder::new(g).literal_separator(true).build() {
                            self.err(at, k, format!("bad glob `{g}`: {e}"));
                        }
                    }
                    b.made_of = v
                }
                ("badges", Some(v)) => b.badges = Some(Field::person(v)),
                ("pattern", Some(v)) => match pattern(&v) {
                    Ok(p) => b.pattern = Some(Field::person(p)),
                    Err(e) => self.err(at, k, e),
                },
                ("at" | "badges" | "pattern", None) => {
                    self.err(at, k, format!("`{kn}` takes strings"))
                }
                _ => {}
            }
        }
        self.boxes.push(b);
        for k in kids {
            let kn = k.name().value();
            if let Some(kind) = named::<ArrowKind>(kn) {
                self.arrow(at, k, &id, kind);
            } else if !["at", "badges", "pattern"].contains(&kn) {
                self.node(at, k, Some(&id), sys);
            }
        }
    }

    fn props(&mut self, at: Src, n: &KdlNode, b: &mut MapBox) {
        if n.entries().iter().filter(|e| e.name().is_none()).count() != 1 {
            self.err(at, n, "expected just the id before the properties".into());
        }
        for e in n.entries().iter().filter(|e| e.name().is_some()) {
            let key = e.name().map(|k| k.value()).unwrap_or_default();
            let Some(v) = e.value().as_string().map(String::from) else {
                self.err(at, n, format!("`{key}` must be a string"));
                continue;
            };
            match key {
                "title" => b.title = Field::person(v),
                "tech" => b.tech = Some(Field::person(v)),
                "description" => b.description = Some(Field::person(v)),
                "logo" => b.logo = Some(Field::person(v)),
                "slot" => b.slot = Some(Field::person(v)),
                _ => self.err(at, n, format!("unknown property `{key}`")),
            }
        }
        if n.get("title").is_none() {
            self.err(at, n, format!("`{}` has no title", b.id));
        }
    }

    fn arrow(&mut self, at: Src, n: &KdlNode, from: &str, kind: ArrowKind) {
        let Some(to) = n.entries().first().and_then(|e| e.value().as_string()) else {
            return self.err(at, n, "an arrow needs the id it points to".into());
        };
        if kind == ArrowKind::Bundle {
            return self.err(
                at,
                n,
                "`bundle` is drawn, not written: write the arrow's kind".into(),
            );
        }
        if to == from {
            return self.err(at, n, format!("arrow `{from}->{to}` points at its own box"));
        }
        let label = n.get("label").and_then(|v| v.as_string()).map(String::from);
        let id = format!("{from}->{to}");
        if self.arrows.iter().any(|(a, _)| a.id == id) {
            self.err(at, n, format!("arrow `{id}` is written twice"));
        }
        let place = format!("{}:{}", at.path, line(at.text, n.span().offset()));
        self.arrows.push((
            MapArrow {
                id,
                parent: String::new(),
                from: from.into(),
                to: to.into(),
                kind,
                label,
                count: 0,
                kinds: vec![kind],
                in_loop: false,
                origin: Default::default(),
            },
            place,
        ));
    }

    fn finish(mut self) -> Result<MapFile, Vec<String>> {
        let parents: BTreeMap<String, Option<String>> = self
            .boxes
            .iter()
            .map(|b| (b.id.clone(), b.parent.clone()))
            .collect();
        let chain = |id: &str| {
            let mut out = vec![];
            let mut id = id.to_string();
            while let Some(Some(p)) = parents.get(&id).filter(|_| out.len() <= parents.len()) {
                out.push(p.clone());
                id = p.clone();
            }
            out
        };
        let mut errs = std::mem::take(&mut self.errs);
        // Duplicate ids make the parent chains unreliable.
        if !errs.is_empty() {
            errs.extend(
                self.arrows
                    .iter()
                    .filter(|(a, _)| !parents.contains_key(&a.to))
                    .map(|(a, place)| format!("{place}: arrow target `{}` does not exist", a.to)),
            );
            return Err(errs);
        }
        let mut arrows = vec![];
        for (mut a, place) in self.arrows {
            if !parents.contains_key(&a.to) {
                errs.push(format!("{place}: arrow target `{}` does not exist", a.to));
                continue;
            }
            let to = chain(&a.to);
            a.parent = chain(&a.from)
                .into_iter()
                .find(|p| to.contains(p))
                .unwrap_or_default();
            arrows.push(a);
        }
        if !errs.is_empty() {
            return Err(errs);
        }
        self.boxes.sort_by(|a, b| a.id.cmp(&b.id));
        arrows.sort_by(|a, b| a.id.cmp(&b.id));
        let m = MapFile {
            version: 1,
            boxes: self.boxes,
            arrows,
            marks: vec![],
        };
        validate_spec(&m).map_err(|e| {
            e.into_iter()
                .map(|x| format!("{DIR}/: {x}"))
                .collect::<Vec<_>>()
        })?;
        Ok(m)
    }
}

pub(crate) fn pattern(v: &[String]) -> Result<Pattern, String> {
    match v.split_first() {
        Some((k, [])) if k == "onion" => Ok(Pattern::Onion),
        Some((k, [])) if k == "hexagonal" => Ok(Pattern::Hexagonal),
        Some((k, [])) if k == "none" => Ok(Pattern::None),
        Some((k, bands)) if k == "layered" && !bands.is_empty() => Ok(Pattern::Layered {
            bands: bands.to_vec(),
        }),
        _ => Err("pattern is onion, hexagonal, none, or layered with its bands".into()),
    }
}
