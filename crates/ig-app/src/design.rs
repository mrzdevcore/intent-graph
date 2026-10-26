//! Features: a folder `plans/<slug>/` (design.json, plan.md, notes.md, screens/, assets/) or a
//! flat plan `plans/<slug>.md`. Design files are user content: written only on the user's
//! action, atomically and never over a file this ig cannot read.

use crate::plan::{self, Head};
use crate::screens::Pieces;
use crate::{Result, bail, read_text};
use ig_core::config::Severity;
use ig_core::design::{self, Design, Scope};
use ig_core::model::Finding;
use ig_core::ports::Files;
use ig_core::rules::Layers;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Feature {
    pub slug: String,
    pub title: String,
    pub design: bool,
    /// The plan file, when there is one.
    pub plan: Option<String>,
    pub done: bool,
    /// A flat plan (`plans/<slug>.md`), no folder.
    pub flat: bool,
}

/// `^[a-z0-9][a-z0-9-]{0,63}$`: feature slugs and screen ids.
pub fn valid_slug(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// "Patient export!" -> "patient-export"; empty when the title has no letter or digit.
pub fn slug_of(title: &str) -> String {
    let s = title
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    s.chars()
        .take(64)
        .collect::<String>()
        .trim_end_matches('-')
        .into()
}

pub fn design_path(slug: &str) -> String {
    format!("plans/{slug}/design.json")
}

fn check_slug(slug: &str) -> Result<()> {
    if !valid_slug(slug) {
        bail!("bad feature name {slug:?}");
    }
    Ok(())
}

/// Every feature, by slug. A folder wins over a flat plan of the same name.
pub fn list(files: &dyn Files) -> Vec<Feature> {
    let pats = ["plans/*/design.json", "plans/*/plan.md", "plans/*.md"].map(String::from);
    let mut by: BTreeMap<String, Feature> = BTreeMap::new();
    for p in files.glob(&pats).unwrap_or_default() {
        let parts: Vec<&str> = p.split('/').collect();
        let (slug, flat) = match parts[..] {
            [_, s, _] => (s, false),
            [_, f] => (f.trim_end_matches(".md"), true),
            _ => continue,
        };
        if !valid_slug(slug) {
            continue;
        }
        let f = by.entry(slug.into()).or_insert_with(|| Feature {
            slug: slug.into(),
            title: slug.into(),
            design: false,
            plan: None,
            done: false,
            flat,
        });
        if f.flat && !flat {
            f.flat = false;
            f.plan = None;
            f.done = false;
        }
        if flat && !f.flat {
            continue;
        }
        if parts.last() == Some(&"design.json") {
            f.design = true;
            if let Ok(d) = design::parse(&read_text(files, &p)) {
                f.title = d.title;
            }
        } else {
            // Read by line, not parsed: a plan with a broken block still is a feature.
            let text = read_text(files, &p);
            let title = text
                .lines()
                .find_map(|l| l.strip_prefix("# "))
                .map(str::trim);
            f.done = text.lines().any(|l| l.trim() == "Status: done");
            f.plan = Some(p);
            if let Some(t) = title.filter(|t| !t.is_empty() && !f.design) {
                f.title = t.into();
            }
        }
    }
    by.into_values().collect()
}

/// Creates `plans/<slug>/design.json` for a title; refuses a slug that is taken.
pub fn create_as(files: &dyn Files, slug: &str, title: &str, scope: Scope) -> Result<Feature> {
    check_slug(slug)?;
    let title = title.trim();
    if title.is_empty() {
        bail!("a feature needs a title");
    }
    if list(files).iter().any(|f| f.slug == slug) || files.exists(&design_path(slug)) {
        bail!("{slug} already exists");
    }
    files.write_atomic(
        &design_path(slug),
        design::to_text(&design::empty(title, scope)).as_bytes(),
    )?;
    Ok(Feature {
        slug: slug.into(),
        title: title.into(),
        design: true,
        plan: None,
        done: false,
        flat: false,
    })
}

pub fn create(files: &dyn Files, title: &str, scope: Scope) -> Result<Feature> {
    let slug = slug_of(title);
    if slug.is_empty() {
        bail!("the title needs a letter or a digit");
    }
    create_as(files, &slug, title, scope)
}

/// `slug_of(title)`, or the first of `-2`, `-3`, ... that no feature has.
pub fn free_slug(files: &dyn Files, title: &str) -> Result<String> {
    let base = slug_of(title);
    if base.is_empty() {
        bail!("the title needs a letter or a digit");
    }
    let taken: Vec<String> = list(files).into_iter().map(|f| f.slug).collect();
    let fits = |s: &String| !taken.contains(s) && !files.exists(&design_path(s));
    let mut cands = std::iter::once(base.clone()).chain((2..).map(|n| {
        let tail = format!("-{n}");
        let head: String = base.chars().take(64 - tail.len()).collect();
        format!("{}{tail}", head.trim_end_matches('-'))
    }));
    Ok(cands.find(fits).expect("endless"))
}

/// The design and the exact bytes it was read from.
pub fn read_raw(files: &dyn Files, slug: &str) -> Result<(Design, Vec<u8>)> {
    check_slug(slug)?;
    let path = design_path(slug);
    let Some(bytes) = files.read_bytes(&path) else {
        bail!("{path}: not found");
    };
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| crate::Error::Other(anyhow::anyhow!("{path}: not UTF-8")))?;
    match design::parse(text) {
        Ok(d) => Ok((d, bytes)),
        Err(e) => bail!("{path}: {e}"),
    }
}

pub fn read(files: &dyn Files, slug: &str) -> Result<Design> {
    read_raw(files, slug).map(|r| r.0)
}

/// Writes `d` unless the disk differs from `expect` (the bytes last read or written; None: no
/// file yet): then false, nothing written. A file that cannot be read is never overwritten.
// ponytail: check-then-rename is not atomic across processes; a lock file if two writers appear.
pub fn write(files: &dyn Files, slug: &str, d: &Design, expect: Option<&[u8]>) -> Result<bool> {
    check_slug(slug)?;
    let path = design_path(slug);
    let now = files.read_bytes(&path);
    if let Some(b) = &now
        && !std::str::from_utf8(b).is_ok_and(|t| design::parse(t).is_ok())
    {
        bail!("design.json unreadable; fix or delete it first");
    }
    if now.as_deref() != expect {
        return Ok(false);
    }
    let text = design::to_text(d);
    if let Err(e) = design::parse(&text) {
        bail!("{e}");
    }
    files.write_atomic(&path, text.as_bytes())?;
    Ok(true)
}

/// Live check of an unsaved design against `head`: the plan check on `to_plan(d)` plus the
/// design's own findings. Every finding gets a `target` (element id, link key, bridge key, null).
/// `screens`: each screen's pieces (`screens::pieces`), for the bridges' sources.
pub fn check(head: &Head, d: &Design, screens: &Pieces, path: &str) -> Result<Value> {
    let planned = design::to_plan(d, path);
    let mut v = plan::check_plan(head, &planned.plan)?;
    let by_graph: HashMap<&str, &str> = planned
        .by_node
        .iter()
        .map(|(e, g)| (g.as_str(), e.as_str()))
        .collect();
    let mut fs: Vec<Finding> = planned.findings.clone();
    let have: HashSet<&str> = head.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    let layers = Layers::new(&head.cfg, &head.packages);
    let mut extra = |rule: &str, sev: Severity, id: &str, msg: String| {
        fs.push(
            Finding::new(rule, sev, "ig", path, 0, msg)
                .node(id)
                .fingerprinted(),
        )
    };
    for e in &d.elements {
        let Some(g) = planned.by_node.get(&e.id) else {
            continue;
        };
        if have.contains(g.as_str()) {
            extra(
                "design/exists",
                Severity::Warning,
                &e.id,
                format!("{g} is already in the graph"),
            );
        }
        if let (Some(row), Some(file)) = (&e.layer, &e.file)
            && let Some(l) = layers.of(file)
            && l != row
        {
            extra(
                "design/layer-mismatch",
                Severity::Warning,
                &e.id,
                format!("placed in {row} but {file} is in {l}"),
            );
        }
    }
    let ids: HashSet<&str> = d.elements.iter().map(|e| e.id.as_str()).collect();
    for b in &d.bridges {
        let key = b.key();
        if !ids.contains(b.to.as_str()) && !have.contains(b.to.as_str()) {
            extra(
                "design/bridge-target-missing",
                Severity::Error,
                &key,
                format!(
                    "bridge {key}: {} is neither an element nor in the graph",
                    b.to
                ),
            );
        }
        if let Some((s, p)) = b.from.split_once('/') {
            match screens.get(s) {
                None => extra(
                    "design/bridge-screen-missing",
                    Severity::Warning,
                    &key,
                    format!("bridge {key}: screen {s} does not exist"),
                ),
                Some(Some(ps)) if !ps.contains(p) => extra(
                    "design/bridge-source-missing",
                    Severity::Warning,
                    &key,
                    format!("bridge {key}: screen {s} has no {p}"),
                ),
                _ => {}
            }
        }
    }
    // An unknown edge end names the link it came from in its message.
    let link_of_message = |m: &str| {
        d.links.iter().find(|l| {
            let (Some(f), Some(t)) = (end(&planned, &ids, &l.from), end(&planned, &ids, &l.to))
            else {
                return false;
            };
            m.starts_with(&format!("edge {f} {} {t}:", l.kind.as_str()))
        })
    };
    let bridge_keys: HashSet<String> = d.bridges.iter().map(|b| b.key()).collect();
    let target = |f: &Finding| -> Value {
        if f.rule_id == "plan/unknown-edge-end" {
            return link_of_message(&f.message).map_or(Value::Null, |l| json!(l.key()));
        }
        if let Some(e) = &f.edge
            && let Some(k) = planned.by_edge.get(&(e.from.clone(), e.to.clone()))
        {
            return json!(k);
        }
        match f.node_id.as_deref() {
            Some(n) if bridge_keys.contains(n) || ids.contains(n) => json!(n),
            Some(n) => by_graph.get(n).map_or(Value::Null, |e| json!(e)),
            None => Value::Null,
        }
    };
    let mut all: Vec<Finding> = serde_json::from_value(v["findings"].take())?;
    all.extend(fs);
    let out: Vec<Value> = all
        .iter()
        .map(|f| {
            let mut j = serde_json::to_value(f).expect("finding");
            j["target"] = target(f);
            j
        })
        .collect();
    v["ok"] = json!(!all.iter().any(|f| f.severity == Severity::Error));
    v["findings"] = Value::Array(out);
    Ok(v)
}

/// The graph id an end of a link stands for: an element's, or the end itself when it is one.
fn end(p: &design::Planned, ids: &HashSet<&str>, s: &str) -> Option<String> {
    if ids.contains(s) {
        p.by_node.get(s).cloned()
    } else {
        Some(s.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ig_core::testing::MemFiles;

    const EMPTY: &str = r#"{"version":1,"title":"A","scope":{"kind":"layer","name":"x"}}"#;
    fn scope() -> Scope {
        Scope::Layer { name: "x".into() }
    }

    #[test]
    fn list_has_folder_and_flat_plans() {
        let f = MemFiles::with(&[
            ("plans/a/design.json", EMPTY),
            ("plans/b/plan.md", "# B\nStatus: done\n"),
            ("plans/c.md", "# C\n"),
            ("plans/c.notes.md", ""),
        ]);
        let l = list(&f);
        let slugs: Vec<_> = l.iter().map(|f| f.slug.as_str()).collect();
        assert_eq!(slugs, ["a", "b", "c"]);
        assert!(l[0].design && l[1].done && l[2].flat && !l[1].flat);
        assert_eq!(l[0].title, "A");
    }

    #[test]
    fn create_writes_an_empty_design_and_refuses_a_second_time() {
        let f = MemFiles::default();
        let c = create(&f, "Patient export!", scope()).unwrap();
        assert_eq!(c.slug, "patient-export");
        assert_eq!(read(&f, "patient-export").unwrap().title, "Patient export!");
        let e = create(&f, "Patient export!", scope()).unwrap_err();
        assert!(e.to_string().contains("already exists"));
        assert_eq!(
            free_slug(&f, "Patient export!").unwrap(),
            "patient-export-2"
        );
        assert!(create(&f, "!!", scope()).is_err());
    }

    #[test]
    fn write_with_stale_expect_writes_nothing() {
        let f = MemFiles::with(&[("plans/a/design.json", EMPTY)]);
        let d = read(&f, "a").unwrap();
        assert!(!write(&f, "a", &d, Some(b"old")).unwrap());
        assert_eq!(
            f.read_bytes("plans/a/design.json").unwrap(),
            EMPTY.as_bytes()
        );
        assert!(!write(&f, "a", &d, None).unwrap());
        assert!(write(&f, "a", &d, Some(EMPTY.as_bytes())).unwrap());
    }

    #[test]
    fn read_reports_invalid_json_and_never_writes() {
        let bad = "<<<<<<< HEAD\n{}\n";
        let f = MemFiles::with(&[("plans/a/design.json", bad)]);
        let e = read(&f, "a").unwrap_err().to_string();
        assert!(e.starts_with("plans/a/design.json: "), "{e}");
        let d = design::empty("A", scope());
        let e = write(&f, "a", &d, Some(bad.as_bytes())).unwrap_err();
        assert_eq!(
            e.to_string(),
            "design.json unreadable; fix or delete it first"
        );
        assert_eq!(f.read_bytes("plans/a/design.json").unwrap(), bad.as_bytes());
    }

    #[test]
    fn bad_slugs_are_refused() {
        let f = MemFiles::default();
        let d = design::empty("A", scope());
        for s in ["../x", "A", "", "a/b", "-a"] {
            assert!(
                read(&f, s).is_err() && write(&f, s, &d, None).is_err(),
                "{s}"
            );
        }
        assert!(f.files.lock().unwrap().is_empty());
    }

    use crate::testing::{Fake, env};
    use ig_core::testing::FakeRunner;

    const CFG: &str =
        "layers:\n  core: ['core/**']\n  ui: ['ui/**']\nrules:\n  - forbid: \"core -> ui\"\n";

    fn head(files: &[(&str, &str)]) -> Head {
        let files = MemFiles::with(files);
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        Head::load(&env(&files, None, &runner, &fe)).unwrap()
    }

    fn dsn(body: &str) -> Design {
        design::parse(&format!(
            r#"{{"version":1,"title":"D","scope":{{"kind":"layer","name":"x"}},{body}}}"#
        ))
        .unwrap()
    }

    fn finding<'a>(v: &'a Value, rule: &str) -> &'a Value {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["ruleId"] == rule)
            .unwrap_or_else(|| panic!("no {rule} in {v}"))
    }

    const FN_N: &str = r#"{"id":"fn-n","kind":"function","name":"n","file":"core/n.ts"}"#;

    #[test]
    fn design_check_marks_the_forbidden_link() {
        let h = head(&[
            ("ig.yaml", CFG),
            ("core/a.ts", "fn a\n"),
            ("ui/b.ts", "fn b\n"),
        ]);
        let d = dsn(&format!(
            r#""elements":[{FN_N}],"links":[{{"from":"fn-n","to":"ui/b.ts#b","kind":"calls"}}]"#
        ));
        let v = check(&h, &d, &Pieces::new(), "plans/f/design.json").unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(finding(&v, "ig/arch")["target"], "fn-n|calls|ui/b.ts#b");
        assert_eq!(v["volume"]["newCrossLayer"][0], json!(["core", "ui"]));
    }

    #[test]
    fn design_check_reports_exists_and_layer_mismatch() {
        let h = head(&[("ig.yaml", CFG), ("core/a.ts", "fn a\n")]);
        let d = dsn(
            r#""elements":[{"id":"mod-a","kind":"module","name":"a","file":"core/a.ts","layer":"ui"}]"#,
        );
        let v = check(&h, &d, &Pieces::new(), "d.json").unwrap();
        assert_eq!(finding(&v, "design/exists")["target"], "mod-a");
        assert_eq!(finding(&v, "design/exists")["severity"], "warning");
        let m = finding(&v, "design/layer-mismatch");
        assert_eq!(m["message"], "placed in ui but core/a.ts is in core");
        assert_eq!(m["target"], "mod-a");
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn design_check_flags_vanished_link_and_bridge_targets() {
        let h = head(&[("ig.yaml", CFG), ("core/a.ts", "fn a\n")]);
        let d = dsn(&format!(
            r#""elements":[{FN_N}],"links":[{{"from":"fn-n","to":"ui/gone.ts#g","kind":"calls"}}],
               "bridges":[{{"from":"list/btn","to":"ui/gone.ts#g"}},{{"from":"list/ok","to":"fn-n"}}]"#
        ));
        let list = |p: &[&str]| {
            Pieces::from([(
                "list".into(),
                Some(p.iter().map(|p| p.to_string()).collect()),
            )])
        };
        let v = check(&h, &d, &list(&["btn", "ok"]), "d.json").unwrap();
        assert!(!v.to_string().contains("bridge-source-missing"), "{v}");
        assert_eq!(
            finding(&v, "plan/unknown-edge-end")["target"],
            "fn-n|calls|ui/gone.ts#g"
        );
        let b = finding(&v, "design/bridge-target-missing");
        assert_eq!(
            (b["target"].as_str(), b["severity"].as_str()),
            (Some("list/btn>ui/gone.ts#g"), Some("error"))
        );
        assert_eq!(v["ok"], false);
        let v = check(&h, &d, &Pieces::new(), "d.json").unwrap();
        assert_eq!(
            finding(&v, "design/bridge-screen-missing")["severity"],
            "warning"
        );
        // the piece a bridge starts from is gone
        let v = check(&h, &d, &list(&["btn"]), "d.json").unwrap();
        let m = finding(&v, "design/bridge-source-missing");
        assert_eq!(
            (m["target"].as_str(), m["severity"].as_str()),
            (Some("list/ok>fn-n"), Some("warning"))
        );
        assert_eq!(m["message"], "bridge list/ok>fn-n: screen list has no ok");
        // a screen file that does not parse: its pieces are unknown, nothing is called missing
        let v = check(&h, &d, &Pieces::from([("list".into(), None)]), "d.json").unwrap();
        assert!(!v.to_string().contains("bridge-source-missing"), "{v}");
    }

    #[test]
    fn design_check_targets_a_duplicate_by_element_id() {
        let h = head(&[("ig.yaml", CFG)]);
        let d = dsn(
            r#""elements":[{"id":"fn-a","kind":"function","name":"n","file":"core/n.ts"},{"id":"fn-b","kind":"function","name":"n","file":"core/n.ts"}]"#,
        );
        let v = check(&h, &d, &Pieces::new(), "d.json").unwrap();
        assert_eq!(finding(&v, "design/duplicate")["target"], "fn-b");
    }

    #[test]
    #[ignore]
    fn design_check_is_fast_on_a_large_graph() {
        let mut src: Vec<(String, String)> = vec![];
        for m in 0..1000 {
            let mut t = String::new();
            for f in 0..4 {
                t += &format!("fn f{f}\n");
                for k in 0..=(m % 2 + 1) {
                    t += &format!("call core/m{}.ts#f{}\n", (m + k + 1) % 1000, k % 4);
                }
            }
            src.push((format!("core/m{m}.ts"), t));
        }
        let mut files: Vec<(&str, &str)> =
            src.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        files.push(("ig.yaml", CFG));
        let h = head(&files);
        assert!(h.graph.edges.len() >= 6000, "{}", h.graph.edges.len());
        let els: Vec<String> = (0..200)
            .map(|i| {
                format!(
                    r#"{{"id":"fn-x{i}","kind":"function","name":"x{i}","file":"core/new{i}.ts"}}"#
                )
            })
            .collect();
        let links: Vec<String> = (0..250)
            .map(|i| {
                format!(
                    r#"{{"from":"fn-x{}","to":"core/m{}.ts#f1","kind":"calls"}}"#,
                    i % 200,
                    i
                )
            })
            .collect();
        let d = dsn(&format!(
            r#""elements":[{}],"links":[{}]"#,
            els.join(","),
            links.join(",")
        ));
        let mut ms: Vec<u128> = (0..20)
            .map(|_| {
                let t = std::time::Instant::now();
                check(&h, &d, &Pieces::new(), "d.json").unwrap();
                t.elapsed().as_millis()
            })
            .collect();
        ms.sort();
        assert!(ms[10] < 120, "median {} ms", ms[10]);
    }
}
