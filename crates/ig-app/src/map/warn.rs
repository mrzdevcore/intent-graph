//! What ig says about a spec that reads but looks wrong: arrows the kinds do not expect,
//! boxes in a place their role does not sit, boxes holding more than a map shows well.
//! Warnings never change `ig check`'s exit code.

use super::file::{MapBox, MapFile, is_unread};
use super::kinds::{KindDef, Kinds, base_name, kind_may_sit};
use super::{ArrowKind, BoxKind, facts::Role};
use std::collections::HashMap;

const CROWDED: usize = 12;

fn name(b: &MapBox) -> String {
    b.custom.clone().unwrap_or_else(|| base_name(b.kind.value))
}

fn def<'a>(kinds: &'a Kinds, b: &MapBox) -> Option<&'a KindDef> {
    kinds.get(&name(b))
}

/// Whether a run block (or a module, which stands for one) is `b` or above it: the reader's test.
fn run_above(by: &HashMap<&str, &MapBox>, b: &MapBox) -> bool {
    let mut at = Some(b);
    while let Some(x) = at {
        if x.kind.value.role() == Role::Run || x.kind.value == BoxKind::Module {
            return true;
        }
        at = x.parent.as_deref().and_then(|p| by.get(p).copied());
    }
    false
}

fn in_box(by: &HashMap<&str, &MapBox>, b: &MapBox) -> bool {
    let mut at = Some(b);
    while let Some(x) = at {
        if x.kind.value == BoxKind::Box {
            return true;
        }
        at = x.parent.as_deref().and_then(|p| by.get(p).copied());
    }
    false
}

/// What is wrong when `b`, as a box of `kind` (named `n`), sits where it does; `None` when it
/// may sit there. `set kind` refuses with it.
pub(crate) fn misplaced(
    by: &HashMap<&str, &MapBox>,
    b: &MapBox,
    kind: BoxKind,
    n: &str,
) -> Option<String> {
    let p = b.parent.as_deref().and_then(|p| by.get(p))?;
    if kind == BoxKind::Box || in_box(by, p) {
        return None;
    }
    if kind_may_sit(kind, p.kind.value.role(), !run_above(by, p)) {
        return None;
    }
    match kind.role() {
        Role::Run => Some(format!(
            "`{}`: a {n} sits in the system or a group outside any running block",
            b.id
        )),
        Role::Design => Some(format!("`{}`: a {n} sits inside a running block", b.id)),
        _ => None,
    }
}

pub fn warnings(m: &MapFile, kinds: &Kinds) -> Vec<String> {
    let by: HashMap<&str, &MapBox> = m.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut out = vec![];
    for a in &m.arrows {
        let (Some(f), Some(t)) = (by.get(a.from.as_str()), by.get(a.to.as_str())) else {
            continue;
        };
        let (Some(fd), Some(td)) = (def(kinds, f), def(kinds, t)) else {
            continue;
        };
        let held = |t: &MapBox| {
            let mut at = vec![t.id.as_str()];
            while let Some(p) = at.pop() {
                for b in m.boxes.iter().filter(|b| b.parent.as_deref() == Some(p)) {
                    if def(kinds, b).is_some_and(|d| kinds.arrow_ok(a.kind, fd, d)) {
                        return true;
                    }
                    at.push(&b.id);
                }
            }
            false
        };
        if a.kind == ArrowKind::Bundle
            || kinds.arrow_ok(a.kind, fd, td)
            || (t.kind.value == BoxKind::Group && held(t))
        {
            continue;
        }
        let k = serde_json::to_value(a.kind).unwrap();
        let k = k.as_str().unwrap_or_default();
        let list = |side: fn(&super::kinds::ArrowRule) -> &Vec<String>| {
            let mut v: Vec<&str> = vec![];
            kinds.rules(a.kind).for_each(|r| {
                side(r).iter().for_each(|n| {
                    if !v.contains(&n.as_str()) {
                        v.push(n)
                    }
                })
            });
            v.join(" or ")
        };
        let from = list(|r| &r.from);
        let from_ok = from.is_empty()
            || kinds.rules(a.kind).any(|r| {
                r.from.is_empty()
                    || r.from
                        .iter()
                        .any(|n| *n == fd.name || *n == base_name(fd.base))
            });
        let ends = if from_ok {
            format!("{k} goes to a {}", list(|r| &r.to))
        } else {
            format!("{k} comes from a {from}")
        };
        out.push(format!("`{}` {k} `{}`: {ends}", a.from, a.to));
    }
    for b in &m.boxes {
        out.extend(misplaced(&by, b, b.kind.value, &name(b)));
    }
    let mut kids: HashMap<&str, usize> = HashMap::new();
    // as the reader once did: stale boxes and a refresh's unread box never count
    (m.boxes.iter())
        .filter(|b| !b.stale)
        .filter_map(|b| b.parent.as_deref().filter(|p| !is_unread(b, p)))
        .for_each(|p| *kids.entry(p).or_default() += 1);
    for b in &m.boxes {
        let n = kids.get(b.id.as_str()).copied().unwrap_or(0);
        if n > CROWDED {
            out.push(format!(
                "`{}` holds {n} boxes; more than {CROWDED} is hard to read",
                b.id
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::spec;

    /// Every node line gets a title, so the tests stay short.
    fn read(kdl: &str) -> MapFile {
        let kdl = kdl
            .replace("\"\n", "\" title=\"T\"\n")
            .replace("\" {\n", "\" title=\"T\" {\n");
        spec::parse(vec![("ig/system.kdl".into(), Some(kdl))]).unwrap()
    }

    #[test]
    fn an_arrow_to_a_group_is_judged_by_what_the_group_holds() {
        let s = "system \"shop\" {\n  service \"api\" {\n    reads \"data\"\n    reads \"people\"\n  }\n  group \"data\" {\n    store \"db\"\n    group \"more\" {\n      queue \"q\"\n    }\n  }\n  group \"people\" {\n    external \"gmail\"\n  }\n}\n";
        let w = warnings(&read(s), Kinds::builtin());
        assert!(!w.iter().any(|w| w.contains("`data`")), "{w:?}");
        assert!(
            w.contains(&"`api` reads `people`: reads goes to a store or queue".to_string()),
            "{w:?}"
        );
    }

    #[test]
    fn mismatched_arrows_and_crowded_boxes_warn() {
        let mut s = String::from(
            "system \"shop\" {\n  service \"api\" {\n    reads \"gmail\"\n  }\n  external \"gmail\"\n  group \"g\" {\n",
        );
        for i in 0..13 {
            s += &format!("    service \"s{i}\"\n");
        }
        s += "  }\n}\n";
        let w = warnings(&read(&s), Kinds::builtin());
        assert!(
            w.contains(&"`api` reads `gmail`: reads goes to a store or queue".to_string()),
            "{w:?}"
        );
        assert!(
            w.contains(&"`g` holds 13 boxes; more than 12 is hard to read".to_string()),
            "{w:?}"
        );
    }

    #[test]
    fn ig_does_not_warn_on_what_it_draws_from_the_code() {
        let (ws, g, cfg) = crate::map::shop::load();
        let b = crate::map::facts(&ws, &g, &cfg);
        let (mut m, _) = crate::map::file::refresh(None, &b, crate::map::arrows(&b, &g), &[]);
        // the shop's infra fills its ports (`implements OrderRepo`, a type import here)
        let fill = "api/infra-prisma->api/application-ports";
        let a = m.arrows.iter_mut().find(|a| a.id == fill).unwrap();
        a.kind = crate::map::arrows::edge_kind("implements", BoxKind::Port).unwrap();
        assert_eq!(warnings(&m, Kinds::builtin()), Vec::<String>::new());
        let k = Kinds::builtin();
        let def = |n: &str| k.get(n).unwrap();
        // code that uses a queue reads it
        let reads = crate::map::arrows::edge_kind("uses", BoxKind::Queue).unwrap();
        assert!(k.arrow_ok(reads, def("service"), def("queue")));
    }

    #[test]
    fn stale_boxes_and_the_unread_box_are_not_counted() {
        let mut s = String::from("system \"shop\" {\n  group \"g\" {\n    group \"g/unread\"\n");
        for i in 0..13 {
            s += &format!("    service \"s{i}\"\n");
        }
        s += "  }\n}\n";
        let mut m = read(&s);
        m.boxes.iter_mut().find(|b| b.id == "s0").unwrap().stale = true;
        let w = warnings(&m, Kinds::builtin());
        assert!(w.iter().all(|w| !w.contains("holds")), "{w:?}");
    }

    #[test]
    fn a_box_in_the_wrong_place_warns() {
        let s = "system \"shop\" {\n  usecase \"u\"\n  service \"a\" {\n    group \"g\" {\n      service \"b\"\n    }\n  }\n}\n";
        let w = warnings(&read(s), Kinds::builtin());
        assert!(
            w.contains(&"`u`: a usecase sits inside a running block".to_string()),
            "{w:?}"
        );
        assert!(
            w.contains(
                &"`b`: a service sits in the system or a group outside any running block"
                    .to_string()
            ),
            "{w:?}"
        );
    }
}
