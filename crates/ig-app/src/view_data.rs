//! The viewer's data (`ig view --json`, and the page `ig view` and `ig serve` build from it):
//! nodes with status, code and lanes, edges, findings, intents, the plan and its state.

use crate::check::{self, Opts, Run};
use crate::lanes::{LaneMap, system_category};
use crate::{Env, plan};
use ig_core::diff::Status;
use ig_core::linediff;
use ig_core::model::{Edge, Node};
use ig_core::ports::{Files, Vcs};
use ig_core::rules::{self, Layers};
use ig_core::workspace;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

const MAX_SLICE_LINES: usize = 300;

/// Node ids of the changed nodes plus their direct neighbours.
pub fn changed_set<'a>(run: &'a Run, all_edges: &[(&'a str, &'a str)]) -> BTreeSet<&'a str> {
    let changed: HashSet<&str> = run.result.changes.keys().map(String::as_str).collect();
    let mut set: BTreeSet<&str> = changed.iter().copied().collect();
    for (f, t) in all_edges {
        if changed.contains(f) {
            set.insert(t);
        }
        if changed.contains(t) {
            set.insert(f);
        }
    }
    set
}

/// Head nodes plus removed base nodes, with their status.
pub fn all_nodes(run: &Run) -> Vec<(&Node, Status)> {
    let mut v: Vec<(&Node, Status)> = run
        .result
        .graph
        .nodes
        .iter()
        .map(|n| (n, run.diff.status(&n.id)))
        .collect();
    if let Some(b) = &run.base_graph {
        v.extend(
            b.nodes
                .iter()
                .filter(|n| run.diff.status(&n.id) == Status::Removed)
                .map(|n| (n, Status::Removed)),
        );
    }
    v
}

/// Head edges plus removed base edges: (edge, status).
pub fn all_edges(run: &Run) -> Vec<(&Edge, Status)> {
    let mut v: Vec<_> = run
        .result
        .graph
        .edges
        .iter()
        .map(|e| {
            (
                e,
                run.diff
                    .edges
                    .get(&e.key())
                    .copied()
                    .unwrap_or(Status::Unchanged),
            )
        })
        .collect();
    if let Some(b) = &run.base_graph {
        v.extend(
            b.edges
                .iter()
                .filter(|e| run.diff.edges.get(&e.key()) == Some(&Status::Removed))
                .map(|e| (e, Status::Removed)),
        );
    }
    v
}

/// Arch-violating (from, to) pairs over the whole head graph.
pub fn violations(run: &Run) -> HashSet<(String, String)> {
    rules::arch(&run.result.graph, &run.config, &run.packages)
        .into_iter()
        .filter_map(|f| f.edge.map(|e| (e.from, e.to)))
        .collect()
}

fn slice(text: &str, start: u32, end: u32) -> String {
    text.lines()
        .skip(start.saturating_sub(1) as usize)
        .take(((end - start + 1) as usize).min(MAX_SLICE_LINES))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Before/after source of changed nodes: the base from `git show`, the head from the tree or the
/// head commit.
pub struct Sources<'r> {
    run: &'r Run,
    files: &'r dyn Files,
    vcs: Option<&'r dyn Vcs>,
    head_ref: Option<String>,
    base_nodes: HashMap<&'r str, &'r Node>,
    before_files: HashMap<String, String>,
    after_files: HashMap<String, String>,
}

/// The code of a node: its new text, its old text, and their line diff when it changed.
pub struct Code {
    pub after: Option<(u32, String)>,
    pub before: Option<(u32, String)>,
    pub diff: Option<Vec<linediff::Line>>,
}

impl<'r> Sources<'r> {
    pub fn new(env: &Env<'r>, run: &'r Run, opts: &Opts) -> Sources<'r> {
        Sources {
            run,
            files: env.files,
            vcs: env.vcs,
            head_ref: opts.head.as_ref().and(run.result.head.clone()),
            base_nodes: run
                .base_graph
                .iter()
                .flat_map(|b| b.nodes.iter())
                .map(|n| (n.id.as_str(), n))
                .collect(),
            before_files: HashMap::new(),
            after_files: HashMap::new(),
        }
    }

    fn after_file(&mut self, file: &str) -> &str {
        let (files, vcs, head) = (self.files, self.vcs, &self.head_ref);
        self.after_files
            .entry(file.to_string())
            .or_insert_with(|| match (head, vcs) {
                (Some(h), Some(r)) => r.show(h, file).unwrap_or_default(),
                _ => crate::read_text(files, file),
            })
    }

    fn before_file(&mut self, file: &str) -> &str {
        let (run, vcs) = (self.run, self.vcs);
        self.before_files
            .entry(file.to_string())
            .or_insert_with(|| match (&run.result.base, vcs) {
                (Some(b), Some(r)) => r.show(b, file).unwrap_or_default(),
                _ => String::new(),
            })
    }

    pub fn code(&mut self, n: &Node, s: Status) -> Code {
        let mut c = Code {
            after: None,
            before: None,
            diff: None,
        };
        if n.is_module() || n.kind == "system" {
            return c;
        }
        if matches!(s, Status::Added | Status::Changed) {
            c.after = Some((
                n.range.start_line,
                slice(
                    self.after_file(&n.file),
                    n.range.start_line,
                    n.range.end_line,
                ),
            ));
        }
        if matches!(s, Status::Changed | Status::Removed)
            && let Some(b) = self.base_nodes.get(n.id.as_str()).copied()
        {
            c.before = Some((
                b.range.start_line,
                slice(
                    self.before_file(&b.file),
                    b.range.start_line,
                    b.range.end_line,
                ),
            ));
        }
        if let (Some((a0, a)), Some((b0, b))) = (&c.before, &c.after) {
            c.diff = Some(linediff::diff(a, *a0, b, *b0));
        }
        c
    }
}

pub fn data(env: &Env, run: &Run, opts: &Opts) -> Value {
    let mut sources = Sources::new(env, run, opts);
    let layers = Layers::new(&run.config, &run.packages);
    let bp = &run.config.blueprint;
    let lanes = LaneMap::new(&run.config, &layers);
    let test_files =
        workspace::globset(&run.config.tests).unwrap_or_else(|_| globset::GlobSet::empty());
    let lane_of = |n: &Node| lanes.of(n);
    let nodes: Vec<Value> = all_nodes(run)
        .into_iter()
        .map(|(n, s)| {
            let mut v = json!({
                "id": n.id, "kind": n.kind, "name": n.name, "file": n.file, "line": n.range.start_line,
                "endLine": n.range.end_line, "parent": n.parent, "status": s, "complexity": n.metrics.complexity,
                "loc": n.metrics.loc, "intents": n.intents, "module": n.is_module(), "layer": layers.of(&n.file), "lane": lane_of(n), "test": test_files.is_match(&n.file),
            });
            if n.kind == "system" {
                let none = vec![];
                let pkgs = bp.systems.iter().find(|(s, _)| *s == n.name).map_or(&none, |(_, p)| p);
                v["category"] = json!(system_category(&n.name, pkgs));
                v["packages"] = json!(pkgs);
            }
            if run.planned.contains(&n.id) {
                v["planned"] = json!(true);
                return v;
            }
            let code = sources.code(n, s);
            match (code.diff, code.after, code.before) {
                (Some(d), _, _) => v["diff"] = json!(d),
                (None, Some((l, t)), _) => v["code"] = json!({"line": l, "text": t, "op": "+"}),
                (None, None, Some((l, t))) => v["code"] = json!({"line": l, "text": t, "op": "-"}),
                _ => {}
            }
            v
        })
        .collect();
    let viol = violations(run);
    let edges: Vec<Value> = all_edges(run)
        .into_iter()
        .map(|(e, s)| {
            let mut v = json!({"from": e.from, "to": e.to, "kind": e.kind, "confidence": e.confidence, "status": s, "line": e.line,
                   "violation": viol.contains(&(e.from.clone(), e.to.clone()))});
            if run.planned.contains(&e.key()) {
                v["planned"] = json!(true);
            }
            v
        })
        .collect();
    let node_ids: HashSet<&str> = run
        .result
        .graph
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .collect();
    let findings: Vec<Value> = run
        .result
        .findings
        .iter()
        .map(|f| {
            let mut v = serde_json::to_value(f).unwrap();
            let pinned = f
                .node_id
                .clone()
                .filter(|n| node_ids.contains(n.as_str()))
                .or_else(|| node_ids.contains(f.file.as_str()).then(|| f.file.clone()));
            v["pinned"] = json!(pinned);
            v
        })
        .collect();
    let tests = workspace::globset(&run.config.tests).unwrap_or_else(|_| globset::GlobSet::empty());
    let intents: Vec<Value> = run
        .intents
        .iter()
        .map(|i| {
            let tagged: Vec<&Node> = run.result.graph.nodes.iter().filter(|n| n.intents.contains(&i.id)).collect();
            let (t, im): (Vec<&&Node>, Vec<&&Node>) = tagged.iter().partition(|n| n.kind == "test" || tests.is_match(&n.file));
            json!({"id": i.id, "text": i.text, "implementedBy": im.iter().map(|n| &n.id).collect::<Vec<_>>(),
                   "testedBy": t.iter().map(|n| &n.id).collect::<Vec<_>>()})
        })
        .collect();
    let pairs: Vec<(&str, &str)> = all_edges(run)
        .into_iter()
        .map(|(e, _)| (e.from.as_str(), e.to.as_str()))
        .collect();
    let set = changed_set(run, &pairs);
    let mode = if run.plan.is_none() && (set.is_empty() || set.len() > 300) {
        "architecture"
    } else {
        "changed"
    };
    let layer_names: Vec<&str> = run.config.layers.iter().map(|(n, _)| n.as_str()).collect();
    let mut out = json!({
        "nodes": nodes, "edges": edges, "findings": findings, "intents": intents,
        "intentChanges": run.result.intent_changes, "notices": run.result.notices,
        "editor": run.config.editor, "root": env.root.to_string_lossy(), "mode": mode, "layers": layer_names,
        "lanes": if bp.lanes.is_empty() { vec![] } else { bp.lanes.iter().map(|l| l.name.as_str()).chain(["Systems"]).collect::<Vec<_>>() },
        "blocking": run.result.blocking, "base": run.result.base, "head": run.result.head, "counts": run.result.counts,
    });
    if let Some(p) = &run.plan {
        out["plan"] = p.clone();
    }
    out["state"] = json!(match &run.plan {
        Some(p) if p["applying"] == true => "applying",
        Some(p) if p["done"] == true => "done",
        Some(_) => "planned",
        None => "plain",
    });
    if let Some(ids) = crate::read_json::<Vec<String>>(env.files, FOCUS) {
        out["focus"] = json!(ids);
    }
    out
}

pub const FOCUS: &str = ".ig/focus.json";

/// The plan to show and whether it is being applied: `--plan`, else the plan of a valid apply
/// state, else `.ig/current-plan`; `--no-plan` shows none. One apply-state read decides both.
pub type Picked = (Option<PathBuf>, bool);

pub fn pick_plan(env: &Env, explicit: Option<&Path>, no_plan: bool) -> Picked {
    if no_plan {
        return (None, false);
    }
    let st = crate::apply::active(env, None);
    let applied = st.as_ref().map(|st| env.root.join(&st.plan));
    let plan = explicit
        .map(Path::to_path_buf)
        .or_else(|| applied.clone())
        .or_else(|| plan::current(env));
    // Applying only when the plan shown is the one being applied.
    let applying = applied.is_some() && plan == applied;
    (plan, applying)
}

/// The plan `pick_plan` chose, read by the caller (plan paths are relative to the working
/// directory): its root-relative path and text, or why it could not be read.
pub struct ShownPlan {
    pub path: PathBuf,
    pub applying: bool,
    pub read: anyhow::Result<(String, String)>,
}

/// `check::run` with `plan` overlaid. A plan that fails to load becomes a notice.
pub fn run_with_plan(env: &Env, opts: &Opts, plan: Option<ShownPlan>) -> crate::Result<Run> {
    let mut run = check::run(env, opts)?;
    let Some(ShownPlan {
        path,
        applying,
        read,
    }) = plan
    else {
        return Ok(run);
    };
    let loaded = read.and_then(|(rel, text)| {
        Ok((
            plan::parse(&rel, &text)?,
            plan::check_text(env, &rel, &text)?,
        ))
    });
    match loaded {
        Ok((p, v)) => {
            plan::overlay(&mut run, &p);
            let shown = run.plan.as_mut().unwrap();
            shown["volume"] = v["volume"].clone();
            shown["findings"] = v["findings"].clone();
            shown["applying"] = json!(applying);
        }
        Err(e) => run
            .result
            .notices
            .push(format!("plan {}: {e:#}", path.display())),
    }
    Ok(run)
}

/// `ig view --json`: the data the page is built from.
pub fn json(env: &Env, opts: &Opts, plan: Option<ShownPlan>) -> crate::Result<Value> {
    let run = run_with_plan(env, opts, plan)?;
    Ok(data(env, &run, opts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::ports::Files;
    use ig_core::testing::{FakeRunner, FakeVcs, MemFiles};

    const PLAN: &str =
        "# P\n\n```ig-plan\nnodes:\n  - id: src/n.ts#n\n    status: new\n    exported: true\n```\n";

    fn shown(path: &str, applying: bool) -> Option<ShownPlan> {
        Some(ShownPlan {
            path: path.into(),
            applying,
            read: Ok((path.into(), PLAN.into())),
        })
    }

    #[test]
    fn state_follows_the_plan_and_the_apply() {
        let files = MemFiles::with(&[("src/a.ts", "fn f\n"), ("plans/p.md", PLAN)]);
        let vcs = FakeVcs {
            head: Some("h".into()),
            commits: ["h".to_string()].into(),
            ..FakeVcs::default()
        };
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        let e = env(&files, Some(&vcs), &runner, &fe);
        let o = Opts::default();
        assert_eq!(pick_plan(&e, None, false), (None, false));
        assert_eq!(json(&e, &o, None).unwrap()["state"], "plain");
        files
            .write_atomic(".ig/current-plan", b"plans/p.md\n")
            .unwrap();
        assert_eq!(
            pick_plan(&e, None, false),
            (Some("./plans/p.md".into()), false)
        );
        let d = json(&e, &o, shown("plans/p.md", false)).unwrap();
        assert_eq!(
            (d["state"].as_str(), d["plan"]["title"].as_str()),
            (Some("planned"), Some("P"))
        );
        let planned = d["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "src/n.ts#n");
        assert_eq!(planned.unwrap()["planned"], true);
        assert_eq!(
            json(&e, &o, shown("plans/p.md", true)).unwrap()["state"],
            "applying"
        );
        crate::apply::start(&e, "plans/p.md", PLAN).unwrap();
        assert_eq!(
            pick_plan(&e, None, false),
            (Some("./plans/p.md".into()), true)
        );
        assert_eq!(pick_plan(&e, None, true), (None, false));
        // A plan that cannot be read is a notice.
        let bad = ShownPlan {
            path: "gone.md".into(),
            applying: false,
            read: Err(anyhow::anyhow!("gone")),
        };
        let d = json(&e, &o, Some(bad)).unwrap();
        assert_eq!(d["state"], "plain");
        assert!(
            d["notices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n == "plan gone.md: gone")
        );
        files.write_atomic(FOCUS, br#"["src/a.ts"]"#).unwrap();
        assert_eq!(json(&e, &o, None).unwrap()["focus"], json!(["src/a.ts"]));
    }
}
