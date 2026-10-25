//! `ig view`: the self-contained HTML viewer. `ig serve` (src/serve.rs) serves it live.

use crate::check::{self, Opts, Run};
use crate::checks;
use crate::diff::Status;
use crate::model::Node;
use crate::plan;
use crate::workspace;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

// The viewer page (viewer/dist/index.html, or a placeholder when it is not built), from build.rs.
const TEMPLATE: &str = include_str!(concat!(env!("OUT_DIR"), "/view.html"));
const MAX_SLICE_LINES: usize = 300;

/// Blueprint lanes: a node goes to the first lane whose globs or layers match; systems go in a
/// last "Systems" lane; test files go nowhere.
pub struct LaneMap<'c> {
    config: &'c crate::config::Config,
    layers: &'c checks::Layers<'c>,
    globs: Vec<(globset::GlobSet, globset::GlobSet)>,
    tests: globset::GlobSet,
}

impl<'c> LaneMap<'c> {
    pub fn new(config: &'c crate::config::Config, layers: &'c checks::Layers<'c>) -> LaneMap<'c> {
        let set =
            |g: &[String]| workspace::globset(g).unwrap_or_else(|_| globset::GlobSet::empty());
        LaneMap {
            config,
            layers,
            globs: config
                .blueprint
                .lanes
                .iter()
                .map(|l| (set(&l.matches), set(&l.exclude)))
                .collect(),
            tests: set(&config.tests),
        }
    }

    pub fn of(&self, n: &Node) -> Option<usize> {
        let lanes = &self.config.blueprint.lanes;
        if n.kind == "system" {
            return Some(lanes.len());
        }
        if self.tests.is_match(&n.file) {
            return None;
        }
        let layer = self.layers.of(&n.file);
        lanes.iter().enumerate().position(|(i, l)| {
            let (inc, exc) = &self.globs[i];
            !exc.is_match(&n.file)
                && (inc.is_match(&n.file) || layer.is_some_and(|x| l.layers.iter().any(|y| y == x)))
        })
    }

    pub fn name(&self, i: Option<usize>) -> String {
        let lanes = &self.config.blueprint.lanes;
        match i {
            Some(i) if i < lanes.len() => lanes[i].name.clone(),
            Some(_) => "Systems".into(),
            None => "Other".into(),
        }
    }
}

#[rustfmt::skip]
const SYSTEM_CATEGORIES: &[(&str, &[&str])] = &[
    ("database", &["postgres", "pg", "mysql", "mariadb", "mongo", "mongoose", "prisma", "typeorm", "knex", "sequelize", "sqlite", "redis", "ioredis", "dynamo", "firestore", "supabase", "cassandra"]),
    // Never plain "elastic": "Elastic APM" is monitoring.
    ("search", &["elastic search", "elasticsearch", "@elastic/elasticsearch", "algolia", "meili", "typesense", "opensearch"]),
    ("monitoring", &["apm", "datadog", "sentry", "newrelic", "opentelemetry", "grafana", "prometheus", "logrocket"]),
    ("auth", &["auth0", "okta", "keycloak", "cognito", "clerk", "firebase-auth", "next-auth", "passport"]),
    ("cms", &["contentful", "strapi", "sanity", "prismic", "storyblok", "directus", "ghost"]),
    ("analytics", &["power bi", "powerbi", "segment", "mixpanel", "amplitude", "google-analytics", "gtag", "matomo", "posthog"]),
    ("queue", &["kafka", "rabbit", "amqp", "sqs", "bull", "nats"]),
    ("storage", &["s3", "@aws-sdk/client-s3", "minio", "cloudinary", "blob"]),
    ("payment", &["stripe", "paypal", "adyen"]),
    ("email", &["sendgrid", "mailgun", "nodemailer", "postmark", "ses"]),
];

/// The icon category of a blueprint system: the first whose keywords occur (case-insensitive) in
/// its name or one of its packages, else "service".
pub fn system_category(name: &str, packages: &[String]) -> &'static str {
    let hay: Vec<String> = std::iter::once(name)
        .chain(packages.iter().map(String::as_str))
        .map(str::to_lowercase)
        .collect();
    SYSTEM_CATEGORIES
        .iter()
        .find(|(_, kws)| kws.iter().any(|k| hay.iter().any(|h| h.contains(k))))
        .map_or("service", |(c, _)| c)
}

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
pub fn all_edges(run: &Run) -> Vec<(&crate::model::Edge, Status)> {
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
    checks::arch(&run.result.graph, &run.config, &run.packages)
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
    root: &'r Path,
    head_ref: Option<String>,
    base_nodes: HashMap<&'r str, &'r Node>,
    before_files: HashMap<String, String>,
    after_files: HashMap<String, String>,
}

/// The code of a node: its new text, its old text, and their line diff when it changed.
pub struct Code {
    pub after: Option<(u32, String)>,
    pub before: Option<(u32, String)>,
    pub diff: Option<Vec<crate::linediff::Line>>,
}

impl<'r> Sources<'r> {
    pub fn new(run: &'r Run, root: &'r Path, opts: &Opts) -> Sources<'r> {
        Sources {
            run,
            root,
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
        let (run, root, head) = (self.run, self.root, &self.head_ref);
        self.after_files
            .entry(file.to_string())
            .or_insert_with(|| match (head, &run.repo) {
                (Some(h), Some(r)) => r.show(h, file).unwrap_or_default(),
                _ => std::fs::read_to_string(root.join(file)).unwrap_or_default(),
            })
    }

    fn before_file(&mut self, file: &str) -> &str {
        let run = self.run;
        self.before_files
            .entry(file.to_string())
            .or_insert_with(|| match (&run.result.base, &run.repo) {
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
        if matches!(s, Status::Changed | Status::Removed) {
            if let Some(b) = self.base_nodes.get(n.id.as_str()).copied() {
                c.before = Some((
                    b.range.start_line,
                    slice(
                        self.before_file(&b.file),
                        b.range.start_line,
                        b.range.end_line,
                    ),
                ));
            }
        }
        if let (Some((a0, a)), Some((b0, b))) = (&c.before, &c.after) {
            c.diff = Some(crate::linediff::diff(a, *a0, b, *b0));
        }
        c
    }
}

pub fn data(run: &Run, root: &Path, opts: &Opts) -> Value {
    let mut sources = Sources::new(run, root, opts);
    let layers = checks::Layers::new(&run.config, &run.packages);
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
        "editor": run.config.editor, "root": root.to_string_lossy(), "mode": mode, "layers": layer_names,
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
    if let Some(ids) = std::fs::read_to_string(root.join(FOCUS))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
    {
        out["focus"] = json!(ids);
    }
    out
}

pub const FOCUS: &str = ".ig/focus.json";

/// The plan to show and whether it is being applied: `--plan`, else the plan of a valid apply
/// state, else `.ig/current-plan`; `--no-plan` shows none. One apply-state read decides both.
pub type Picked = (Option<PathBuf>, bool);

pub fn pick_plan(root: &Path, explicit: Option<&Path>, no_plan: bool) -> Picked {
    if no_plan {
        return (None, false);
    }
    let st = crate::apply::active(root, None);
    let applied = st.as_ref().map(|st| root.join(&st.plan));
    let plan = explicit
        .map(Path::to_path_buf)
        .or_else(|| applied.clone())
        .or_else(|| plan::current(root));
    // Applying only when the plan shown is the one being applied.
    let applying = applied.is_some() && plan == applied;
    (plan, applying)
}

pub fn render(run: &Run, root: &Path, opts: &Opts) -> String {
    page(&data(run, root, opts), None)
}

/// The page for `data`. Served by `ig serve`, it gets the API token (live); a file gets none.
pub fn page(data: &Value, token: Option<&str>) -> String {
    let live = token.map_or("false".into(), |t| json!({ "token": t }).to_string());
    fill(TEMPLATE, &data.to_string().replace('<', "\\u003c"), &live)
}

fn fill(template: &str, data_json: &str, live: &str) -> String {
    template
        .replace("/*IG_LIVE*/", live)
        .replace("/*IG_DATA*/", data_json)
}

/// `check::run` with the plan at `plan` overlaid. A plan that fails to load becomes a notice.
pub fn run_with_plan(root: &Path, opts: &Opts, (plan, applying): &Picked) -> Result<Run> {
    let mut run = check::run(root, opts)?;
    let Some(path) = plan else { return Ok(run) };
    let path = path.as_path();
    let loaded = plan::read(root, path).and_then(|(rel, text)| {
        Ok((
            plan::parse(&rel, &text)?,
            plan::check_text(root, &rel, &text)?,
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
pub fn json(root: &Path, opts: &Opts, plan: &Picked) -> Result<Value> {
    let run = run_with_plan(root, opts, plan)?;
    Ok(data(&run, root, opts))
}

pub fn write(root: &Path, opts: &Opts, plan: &Picked) -> Result<PathBuf> {
    let run = run_with_plan(root, opts, plan)?;
    let html = render(&run, root, opts);
    let path = root.join(".ig/view.html");
    check::write_atomic(&path, html.as_bytes())?;
    let _ = std::fs::remove_file(root.join(FOCUS)); // a focus is shown once
    Ok(path)
}

pub fn write_run(root: &Path, run: &Run, opts: &Opts) -> Result<PathBuf> {
    let path = root.join(".ig/view.html");
    check::write_atomic(&path, render(run, root, opts).as_bytes())?;
    Ok(path)
}

pub fn open(target: &Path) {
    open_str(&target.to_string_lossy());
}

pub fn open_str(target: &str) {
    let cmd = if cfg!(target_os = "macos") {
        vec!["open", target]
    } else if cfg!(windows) {
        vec!["cmd", "/C", "start", "", target]
    } else {
        vec!["xdg-open", target]
    };
    let _ = std::process::Command::new(cmd[0])
        .args(&cmd[1..])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn ignored(root: &Path, p: &Path) -> bool {
    let rel = p.strip_prefix(root).unwrap_or(p);
    rel.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some(".ig" | ".git" | "node_modules")
        )
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn page_fills_the_markers_in_the_viewer_and_the_placeholder() {
        let data = r#"{"nodes":[],"x":"\u003c/script>"}"#;
        let placeholder = include_str!(concat!(env!("OUT_DIR"), "/placeholder.html"));
        let live = page(&json!({"nodes": [], "x": "</script>"}), Some("tok"));
        assert!(live.contains(r#"{"token":"tok"}"#));
        for (name, html) in [
            ("view", live),
            ("placeholder", fill(placeholder, data, "false")),
        ] {
            assert!(html.contains(data), "{name}: data missing");
            assert!(!html.contains("/*IG_DATA*/"), "{name}: data marker left");
            assert!(!html.contains("/*IG_LIVE*/"), "{name}: live marker left");
        }
    }

    pub(crate) fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ig-view-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
        assert!(
            std::process::Command::new("cp")
                .arg("-R")
                .arg(&src)
                .arg(&dir)
                .status()
                .unwrap()
                .success()
        );
        dir.canonicalize().unwrap()
    }

    fn state(dir: &Path) -> (Picked, Value) {
        let p = pick_plan(dir, None, false);
        let d = json(dir, &Opts::default(), &p).unwrap();
        (p, d["state"].clone())
    }

    #[test]
    fn state_and_focus() {
        let dir = scratch("state");
        let o = Opts::default();
        assert_eq!(state(&dir).1, "plain");
        assert!(
            json(&dir, &o, &pick_plan(&dir, None, false))
                .unwrap()
                .get("focus")
                .is_none()
        );
        std::fs::create_dir_all(dir.join("plans")).unwrap();
        std::fs::write(dir.join("plans/export.md"), crate::plan::tests::PLAN).unwrap();
        crate::plan::check(&dir, &dir.join("plans/export.md")).unwrap();
        assert_eq!(state(&dir).1, "planned");
        assert_eq!(pick_plan(&dir, None, true), (None, false));
        std::fs::create_dir_all(dir.join(".ig")).unwrap();
        std::fs::write(dir.join(FOCUS), r#"["a.ts","b.ts"]"#).unwrap();
        let p = pick_plan(&dir, None, false);
        assert_eq!(
            json(&dir, &o, &p).unwrap()["focus"],
            json!(["a.ts", "b.ts"])
        );
        write(&dir, &o, &p).unwrap();
        assert!(!dir.join(FOCUS).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_state_picks_its_plan_until_stale_and_done_follows_the_plan() {
        let dir = scratch("apply");
        let git = |a: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args(a)
                    .current_dir(&dir)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        git(&["init", "-q"]);
        git(&["add", "-A"]);
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "i",
        ]);
        let head = crate::git::git(&dir, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        std::fs::create_dir_all(dir.join("plans")).unwrap();
        std::fs::write(dir.join("plans/export.md"), crate::plan::tests::PLAN).unwrap();
        std::fs::write(dir.join("plans/other.md"), crate::plan::tests::PLAN).unwrap();
        crate::plan::check(&dir, &dir.join("plans/export.md")).unwrap();
        std::fs::write(
            dir.join(".ig/apply.json"),
            json!({"plan": "plans/other.md", "base": head, "hashes": {}}).to_string(),
        )
        .unwrap();
        let (p, s) = state(&dir);
        assert_eq!(
            (p, s),
            ((Some(dir.join("plans/other.md")), true), json!("applying"))
        );
        assert_eq!(pick_plan(&dir, None, true), (None, false));
        let nop = pick_plan(&dir, None, true);
        assert_eq!(
            json(&dir, &Opts::default(), &nop).unwrap()["state"],
            "plain"
        );
        let other = pick_plan(&dir, Some(&dir.join("plans/export.md")), false);
        assert_eq!(
            json(&dir, &Opts::default(), &other).unwrap()["state"],
            "planned"
        );
        std::fs::remove_file(dir.join("plans/other.md")).unwrap();
        let (p, s) = state(&dir);
        assert_eq!(
            (p, s),
            ((Some(dir.join("plans/export.md")), false), json!("planned"))
        );
        std::fs::remove_file(dir.join(".ig/apply.json")).unwrap();
        std::fs::write(
            dir.join("plans/export.md"),
            crate::plan::tests::PLAN.replace(
                "# Export
",
                "# Export
Status: done
",
            ),
        )
        .unwrap();
        assert_eq!(state(&dir).1, "done");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn system_categories() {
        let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(system_category("PostgreSQL", &p(&["pg"])), "database");
        assert_eq!(system_category("MongoDB", &p(&["mongoose"])), "database");
        assert_eq!(
            system_category("Elastic APM", &p(&["elastic-apm-node"])),
            "monitoring"
        );
        assert_eq!(
            system_category("Search", &p(&["@elastic/elasticsearch"])),
            "search"
        );
        assert_eq!(
            system_category("Auth0", &p(&["@auth0/auth0-react"])),
            "auth"
        );
        assert_eq!(system_category("Contentful", &p(&["contentful"])), "cms");
        assert_eq!(
            system_category("Power BI", &p(&["powerbi-client"])),
            "analytics"
        );
        assert_eq!(system_category("Maite BFF", &p(&["@maite/sdk"])), "service");
    }

    #[test]
    fn system_nodes_carry_category_and_packages() {
        let dir = scratch("systems");
        let mut cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap();
        cfg.push_str("blueprint:\n  systems:\n    PostgreSQL: [pg]\n");
        std::fs::write(dir.join("ig.yaml"), cfg).unwrap();
        std::fs::write(
            dir.join("packages/core/src/db.ts"),
            "import { Pool } from \"pg\";\nexport const pool = new Pool();\n",
        )
        .unwrap();
        let d = json(&dir, &Opts::default(), &(None, false)).unwrap();
        let n = d["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "system:PostgreSQL")
            .unwrap();
        assert_eq!(
            (n["category"].clone(), n["packages"].clone()),
            (json!("database"), json!(["pg"]))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn planned_nodes_and_plan_panel() {
        let dir = std::env::temp_dir().join(format!("ig-view-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
        assert!(
            std::process::Command::new("cp")
                .arg("-R")
                .arg(&src)
                .arg(&dir)
                .status()
                .unwrap()
                .success()
        );
        let dir = dir.canonicalize().unwrap();
        let mut run = check::run(&dir, &Opts::default()).unwrap();
        let plan = crate::plan::parse("plans/export.md", crate::plan::tests::PLAN).unwrap();
        crate::plan::overlay(&mut run, &plan);
        let d = data(&run, &dir, &Opts::default());
        let n = d["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "packages/core/src/export.ts#exportRows")
            .unwrap();
        assert_eq!(
            (n["planned"].clone(), n["status"].clone()),
            (json!(true), json!("added"))
        );
        assert_eq!(d["plan"]["title"], "Export");
        assert!(render(&run, &dir, &Opts::default()).contains("planned"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn view_reuses_saved_checker_findings() {
        let dir = std::env::temp_dir().join(format!("ig-view-checkers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
        assert!(
            std::process::Command::new("cp")
                .arg("-R")
                .arg(&src)
                .arg(&dir)
                .status()
                .unwrap()
                .success()
        );
        let dir = dir.canonicalize().unwrap();
        let cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap()
            + "checkers:\n  - name: t\n    format: lines\n    run: \"touch marker; echo 'packages/core/src/math.ts:1: boom'\"\n";
        std::fs::write(dir.join("ig.yaml"), cfg).unwrap();
        let view = Opts {
            skip_checkers: true,
            ..Default::default()
        };
        let boom = |v: &Value| {
            v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["tool"] == "t" && f["message"] == "boom")
        };
        write(&dir, &view, &(None, false)).unwrap();
        assert!(!dir.join("marker").exists(), "view ran a checker");
        check::run(&dir, &Opts::default()).unwrap();
        assert!(dir.join("marker").exists(), "check did not run the checker");
        std::fs::remove_file(dir.join("marker")).unwrap();
        let d = json(&dir, &view, &(None, false)).unwrap();
        assert!(!dir.join("marker").exists(), "view ran a checker");
        assert!(boom(&d), "{}", d["findings"]);
        let saved: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join(".ig/findings.json")).unwrap())
                .unwrap();
        assert!(boom(&saved), "view build dropped the saved checker finding");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checker_findings_on_changed_non_graph_files_survive_view_builds() {
        let dir = scratch("ckscope");
        let cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap()
            + "checkers:\n  - name: t\n    format: lines\n    run: \"echo 'a.css:1: boom'\"\n";
        std::fs::write(dir.join("ig.yaml"), cfg).unwrap();
        std::fs::write(dir.join(".gitignore"), ".ig/\n").unwrap();
        let git = |a: &[&str]| {
            let mut c = vec!["-c", "user.name=t", "-c", "user.email=t@t"];
            c.extend(a);
            assert!(
                std::process::Command::new("git")
                    .args(&c)
                    .current_dir(&dir)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        git(&["init", "-q"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "i"]);
        std::fs::write(dir.join("a.css"), "a { color: red }\n").unwrap();
        let boom = |v: &Value| {
            v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["tool"] == "t" && f["file"] == "a.css")
        };
        let saved = || -> Value {
            serde_json::from_str(&std::fs::read_to_string(dir.join(".ig/findings.json")).unwrap())
                .unwrap()
        };
        // An untracked .css file is in the change: its checker finding counts.
        check::run(&dir, &Opts::default()).unwrap();
        assert!(
            boom(&saved()),
            "finding on a changed non-graph file dropped"
        );

        // Committed: out of scope now. A malformed saved entry must not hide the others.
        git(&["add", "a.css"]);
        git(&["commit", "-qm", "css"]);
        let mut v = saved();
        v["findings"]
            .as_array_mut()
            .unwrap()
            .insert(0, json!({"bogus": 1}));
        std::fs::write(dir.join(".ig/findings.json"), v.to_string()).unwrap();
        let view = Opts {
            skip_checkers: true,
            ..Default::default()
        };
        let d = json(&dir, &view, &(None, false)).unwrap();
        assert!(!boom(&d), "out-of-scope finding shown");
        assert!(boom(&saved()), "view build dropped a saved checker finding");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
