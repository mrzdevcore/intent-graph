//! The building blocks: every kind a box in `ig/` may be, defined in `assets/kinds.kdl`, plus a
//! team's own kinds in `ig/kinds/*.kdl`, each extending a built-in one. Where a kind may sit is
//! not written here: it follows its base's `role()`.

use crate::map::facts::Role;
use crate::map::spec::DIR;
use crate::map::spec::read::{doc, line, named};
use crate::map::{ArrowKind, BoxKind};
use ig_core::ports::Files;
use kdl::KdlNode;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq)]
pub struct PropDef {
    pub name: String,
    pub means: String,
    pub required: bool,
    /// The values it may take; empty = any string.
    pub one_of: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KindDef {
    pub name: String,
    pub base: BoxKind,
    /// A team's kind, from `ig/kinds/`.
    pub custom: bool,
    pub title: String,
    pub means: String,
    pub icon: String,
    /// Its base's, then its own.
    pub props: Vec<PropDef>,
}

/// An arrow of `kind` joins these kinds (by name, a team kind also by its base's); empty = any.
#[derive(Debug, Clone, PartialEq)]
pub struct ArrowRule {
    pub kind: ArrowKind,
    pub from: Vec<String>,
    pub to: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Kinds {
    /// Built-in first, in file order.
    kinds: Vec<KindDef>,
    arrows: Vec<ArrowRule>,
}

impl Kinds {
    pub fn builtin() -> &'static Kinds {
        static K: OnceLock<Kinds> = OnceLock::new();
        K.get_or_init(|| {
            let mut k = Kinds::default();
            let errs = k.add(
                "assets/kinds.kdl",
                include_str!("../../assets/kinds.kdl"),
                false,
            );
            assert!(errs.is_empty(), "{}", errs.join("\n"));
            k
        })
    }

    /// The built-in kinds and the team's in `ig/kinds/*.kdl`; every error, with file and line.
    pub fn load(files: &dyn Files) -> Result<Kinds, Vec<String>> {
        let paths = files
            .glob(&[format!("{DIR}/kinds/*.kdl")])
            .map_err(|e| vec![format!("{DIR}/kinds: {e}")])?;
        let texts = paths.into_iter().map(|p| {
            let t = files.read(&p);
            (p, t)
        });
        match Kinds::team(texts.collect()) {
            (k, errs) if errs.is_empty() => Ok(k),
            (_, errs) => Err(errs),
        }
    }

    /// The built-in kinds and the team's in these (path, text; none when it cannot be read),
    /// with every error: the kinds of the files that read are kept.
    pub fn team(texts: Vec<(String, Option<String>)>) -> (Kinds, Vec<String>) {
        let mut k = Kinds::builtin().clone();
        let mut errs = vec![];
        for (p, text) in texts {
            match text {
                Some(text) => errs.extend(k.add(&p, &text, true)),
                None => errs.push(format!("{p}: cannot be read")),
            }
        }
        (k, errs)
    }

    pub fn get(&self, name: &str) -> Option<&KindDef> {
        self.kinds.iter().find(|k| k.name == name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.kinds.iter().map(|k| k.name.as_str()).collect()
    }

    /// What every kind may take, besides its own properties.
    pub fn common_props() -> &'static [&'static str] {
        &[
            "title",
            "description",
            "tech",
            "at",
            "owner",
            "logo",
            "slot",
            "badges",
            "pattern",
        ]
    }

    /// An arrow kind with no rule joins any two kinds; with rules, one of them must match.
    pub fn arrow_ok(&self, kind: ArrowKind, from: &KindDef, to: &KindDef) -> bool {
        let is = |names: &[String], d: &KindDef| {
            names.is_empty()
                || names
                    .iter()
                    .any(|n| *n == d.name || *n == base_name(d.base))
        };
        let mut rules = self.arrows.iter().filter(|r| r.kind == kind).peekable();
        rules.peek().is_none() || rules.any(|r| is(&r.from, from) && is(&r.to, to))
    }

    /// Adds the kinds (and, built in, the arrow rules) of one file; its errors.
    fn add(&mut self, p: &str, text: &str, team: bool) -> Vec<String> {
        let doc = match doc(p, text) {
            Ok(d) => d,
            Err(e) => return e,
        };
        let mut errs = vec![];
        let mut err = |n: &KdlNode, what: String| {
            errs.push(format!("{p}:{}: {what}", line(text, n.span().offset())))
        };
        for n in doc.nodes() {
            match n.name().value() {
                "arrow" if team => err(n, "a kind cannot add arrows".into()),
                "arrow" => match self.rule(n) {
                    Ok(r) => self.arrows.push(r),
                    Err(e) => err(n, e),
                },
                "kind" => match self.kind(n, team) {
                    Ok(k) => self.kinds.push(k),
                    Err(e) => e.into_iter().for_each(|(n, e)| err(n, e)),
                },
                other => err(n, format!("unknown `{other}`, expected `kind`")),
            }
        }
        errs
    }

    fn rule(&self, n: &KdlNode) -> Result<ArrowRule, String> {
        keys(n, &["from", "to"])?;
        let name = first(n).ok_or("an arrow rule needs the arrow kind")?;
        let kind = named(&name).ok_or(format!("`{name}` is not an arrow kind"))?;
        let from = n.get("from").and_then(|v| v.as_string()).map(String::from);
        Ok(ArrowRule {
            kind,
            from: from.into_iter().collect(),
            to: list(n, "to"),
        })
    }

    /// A kind node, or its errors and the node each is on.
    fn kind<'a>(&self, n: &'a KdlNode, team: bool) -> Result<KindDef, Vec<(&'a KdlNode, String)>> {
        let fail = |e: String| vec![(n, e)];
        keys(n, &["is", "title", "means", "icon"]).map_err(fail)?;
        let name = first(n).ok_or_else(|| fail("a kind needs its name".into()))?;
        let s = |k: &str| n.get(k).and_then(|v| v.as_string()).map(String::from);
        let (base, inherited) = if team {
            if named::<BoxKind>(&name).is_some() {
                return Err(fail(format!("`{name}` is a built-in kind")));
            }
            if self.get(&name).is_some() {
                return Err(fail(format!("`{name}` is defined twice")));
            }
            let is =
                s("is").ok_or_else(|| fail(format!("`{name}` needs is=\"<built-in kind>\"")))?;
            let base = match named::<BoxKind>(&is) {
                Some(b) if b.code_level() => {
                    return Err(fail(format!(
                        "`{is}` is a code-level kind, not one to extend"
                    )));
                }
                Some(b) => b,
                None => return Err(fail(format!("`{is}` is not a built-in kind"))),
            };
            (base, Kinds::builtin().get(&base_name(base)).cloned())
        } else {
            match named::<BoxKind>(&name) {
                Some(b) if !b.code_level() && s("is").is_none() => (b, None),
                _ => return Err(fail(format!("`{name}` is not a spec box kind"))),
            }
        };
        let mut props = inherited
            .as_ref()
            .map(|k| k.props.clone())
            .unwrap_or_default();
        let mut errs = vec![];
        for c in n.children().map(|c| c.nodes()).unwrap_or_default() {
            match (c.name().value(), first(c)) {
                ("arrow", _) => errs.push((c, "a kind cannot add arrows".into())),
                ("property", Some(p)) => match keys(c, &["means", "required", "one-of"]) {
                    Ok(()) => props.push(PropDef {
                        name: p,
                        means: c
                            .get("means")
                            .and_then(|v| v.as_string())
                            .unwrap_or_default()
                            .into(),
                        required: c.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                        one_of: list(c, "one-of"),
                    }),
                    Err(e) => errs.push((c, e)),
                },
                ("property", None) => errs.push((c, "a property needs its name".into())),
                (other, _) => errs.push((c, format!("unknown `{other}` in a kind"))),
            }
        }
        if !errs.is_empty() {
            return Err(errs);
        }
        let or = |k: &str, from: fn(&KindDef) -> &String| {
            s(k).or_else(|| inherited.as_ref().map(|d| from(d).clone()))
                .unwrap_or_default()
        };
        Ok(KindDef {
            title: or("title", |d| &d.title),
            means: or("means", |d| &d.means),
            icon: or("icon", |d| &d.icon),
            name,
            base,
            custom: team,
            props,
        })
    }
}

/// A box of role `child` may sit in one of role `parent` (`parent_outer`: no run block at or
/// above the parent). Run blocks sit in the system or an outer group; design blocks in a run
/// block, a design block or an inner group; a group anywhere but code, holding what may sit
/// where it sits. `kind_may_sit` adds the one kind with a place of its own.
pub fn may_sit(child: Role, parent: Role, parent_outer: bool) -> bool {
    match child {
        Role::Run => parent == Role::Root || (parent == Role::Group && parent_outer),
        Role::Design => {
            matches!(parent, Role::Run | Role::Design) || (parent == Role::Group && !parent_outer)
        }
        Role::Group => parent != Role::Code,
        Role::Root | Role::Code => false,
    }
}

/// `may_sit` for a kind: a `module` also sits where run blocks sit (a library package is one).
pub fn kind_may_sit(child: BoxKind, parent: Role, parent_outer: bool) -> bool {
    may_sit(child.role(), parent, parent_outer)
        || (child == BoxKind::Module && may_sit(Role::Run, parent, parent_outer))
}

pub(crate) fn base_name(b: BoxKind) -> String {
    serde_json::to_value(b)
        .unwrap()
        .as_str()
        .unwrap_or_default()
        .into()
}

/// The node's first string argument: its name.
fn first(n: &KdlNode) -> Option<String> {
    n.entries()
        .first()
        .filter(|e| e.name().is_none())?
        .value()
        .as_string()
        .map(String::from)
}

/// `key="a" "b" "c"`: the property's value and the strings after the name.
fn list(n: &KdlNode, key: &str) -> Vec<String> {
    let rest = n.entries().iter().filter(|e| e.name().is_none()).skip(1);
    n.get(key)
        .into_iter()
        .chain(rest.map(|e| e.value()))
        .filter_map(|v| v.as_string().map(String::from))
        .collect()
}

fn keys(n: &KdlNode, allowed: &[&str]) -> Result<(), String> {
    match n
        .entries()
        .iter()
        .filter_map(|e| e.name())
        .find(|k| !allowed.contains(&k.value()))
    {
        Some(k) => Err(format!("unknown property `{}`", k.value())),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
