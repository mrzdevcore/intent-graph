//! `ig view`: the self-contained HTML viewer, and `--watch` (tiny_http + notify + SSE).

use crate::check::{self, Opts, Run};
use crate::checks;
use crate::diff::Status;
use crate::model::Node;
use crate::plan;
use crate::workspace;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

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
    let applying =
        crate::apply::load(root).is_some_and(|st| crate::apply::stale(root, &st).is_none());
    out["state"] = json!(match &run.plan {
        _ if applying => "applying",
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

const FOCUS: &str = ".ig/focus.json";

/// The plan to show and whether it is being applied: `--plan`, else the plan of a valid apply
/// state, else `.ig/current-plan`; `--no-plan` shows none. One apply-state read decides both.
pub type Picked = (Option<PathBuf>, bool);

pub fn pick_plan(root: &Path, explicit: Option<&Path>, no_plan: bool) -> Picked {
    if no_plan {
        return (None, false);
    }
    let st = crate::apply::load(root).filter(|st| crate::apply::stale(root, st).is_none());
    let plan = explicit
        .map(Path::to_path_buf)
        .or_else(|| st.as_ref().map(|st| root.join(&st.plan)))
        .or_else(|| plan::current(root));
    (plan, st.is_some())
}

pub fn render(run: &Run, root: &Path, opts: &Opts, live: bool) -> String {
    let d = data(run, root, opts).to_string().replace('<', "\\u003c");
    page(&d, live)
}

fn page(data_json: &str, live: bool) -> String {
    fill(TEMPLATE, data_json, live)
}

fn fill(template: &str, data_json: &str, live: bool) -> String {
    template
        .replace("/*IG_LIVE*/", if live { "true" } else { "false" })
        .replace("/*IG_DATA*/", data_json)
}

/// `check::run` with the plan at `plan` overlaid. A plan that fails to load becomes a notice.
fn run_with_plan(root: &Path, opts: &Opts, (plan, applying): &Picked) -> Result<Run> {
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
    let html = render(&run, root, opts, false);
    let path = root.join(".ig/view.html");
    check::write_atomic(&path, html.as_bytes())?;
    let _ = std::fs::remove_file(root.join(FOCUS)); // a focus is shown once
    Ok(path)
}

pub fn write_run(root: &Path, run: &Run, opts: &Opts) -> Result<PathBuf> {
    let path = root.join(".ig/view.html");
    check::write_atomic(&path, render(run, root, opts, false).as_bytes())?;
    Ok(path)
}

pub fn open(target: &Path) {
    open_str(&target.to_string_lossy());
}

fn open_str(target: &str) {
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

fn ignored(root: &Path, p: &Path) -> bool {
    let rel = p.strip_prefix(root).unwrap_or(p);
    rel.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some(".ig" | ".git" | "node_modules")
        )
    })
}

type Clients = Arc<Mutex<Vec<Box<dyn Write + Send>>>>;

pub fn watch(
    root: &Path,
    opts: &Opts,
    plan: Option<PathBuf>,
    no_plan: bool,
    open_browser: bool,
) -> Result<()> {
    use notify::Watcher;
    let opts = opts.clone();
    let build = move |root: &Path| -> Result<String> {
        let opts = &opts;
        // Re-pick the plan on every rebuild so the view follows plan and apply changes.
        let run = run_with_plan(
            root,
            opts,
            &pick_plan(root, plan.as_deref(), no_plan),
        )?;
        let html = render(&run, root, opts, true);
        let _ = check::write_atomic(
            &root.join(".ig/view.html"),
            render(&run, root, opts, false).as_bytes(),
        );
        let _ = std::fs::remove_file(root.join(FOCUS)); // a focus is shown once
        Ok(html)
    };
    let page_html: Arc<Mutex<String>> = Arc::new(Mutex::new(build(root)?));
    let clients: Clients = Arc::new(Mutex::new(vec![]));
    let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| anyhow::anyhow!("{e}"))?;
    let addr = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| anyhow::anyhow!("no address"))?;
    let url = format!("http://{addr}/");
    eprintln!("ig: serving {url} (Ctrl-C to stop)");
    if open_browser {
        open_str(&url);
    }

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |e| {
        let _ = tx.send(e);
    })?;
    watcher.watch(root, notify::RecursiveMode::Recursive)?;
    {
        let (root, page_html, clients, build) = (
            root.to_path_buf(),
            page_html.clone(),
            clients.clone(),
            build.clone(),
        );
        std::thread::spawn(move || {
            let relevant = |e: &notify::Result<notify::Event>| {
                e.as_ref()
                    .is_ok_and(|e| e.paths.iter().any(|p| !ignored(&root, p)))
            };
            while let Ok(e) = rx.recv() {
                if !relevant(&e) {
                    continue;
                }
                // Debounce: wait for 300 ms of quiet.
                while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
                match build(&root) {
                    Ok(html) => {
                        *page_html.lock().unwrap() = html;
                        let mut cs = clients.lock().unwrap();
                        cs.retain_mut(|w| {
                            w.write_all(b"data: reload\n\n")
                                .and_then(|_| w.flush())
                                .is_ok()
                        });
                        eprintln!("ig: rebuilt");
                    }
                    Err(err) => {
                        eprintln!("ig: rebuild failed, keeping the last good page: {err:#}")
                    }
                }
            }
        });
    }
    for req in server.incoming_requests() {
        match req.url() {
            "/events" => {
                let mut w = req.into_writer();
                let ok = w
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n: connected\n\n")
                    .and_then(|_| w.flush())
                    .is_ok();
                if ok {
                    clients.lock().unwrap().push(w);
                }
            }
            "/" | "/index.html" => {
                let html = page_html.lock().unwrap().clone();
                let h = tiny_http::Header::from_bytes("Content-Type", "text/html; charset=utf-8")
                    .unwrap();
                let _ = req.respond(tiny_http::Response::from_string(html).with_header(h));
            }
            _ => {
                let _ = req
                    .respond(tiny_http::Response::from_string("not found").with_status_code(404));
            }
        }
    }
    drop(watcher);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_fills_the_markers_in_the_viewer_and_the_placeholder() {
        let data = r#"{"nodes":[],"x":"\u003c/script>"}"#;
        let placeholder = include_str!(concat!(env!("OUT_DIR"), "/placeholder.html"));
        for (name, html) in [
            ("view", page(data, true)),
            ("placeholder", fill(placeholder, data, false)),
        ] {
            assert!(html.contains(data), "{name}: data missing");
            assert!(!html.contains("/*IG_DATA*/"), "{name}: data marker left");
            assert!(!html.contains("/*IG_LIVE*/"), "{name}: live marker left");
        }
    }

    fn scratch(tag: &str) -> PathBuf {
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
        assert!(render(&run, &dir, &Opts::default(), false).contains("planned"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
