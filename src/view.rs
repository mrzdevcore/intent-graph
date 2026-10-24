//! `ig view`: the self-contained HTML viewer, and `--watch` (tiny_http + notify + SSE).

use crate::check::{self, Opts, Run};
use crate::checks;
use crate::diff::Status;
use crate::model::Node;
use crate::workspace;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const TEMPLATE: &str = include_str!("../assets/view.html");
const SCRIPT: &str = include_str!("../assets/view.js");
const CYTOSCAPE: &str = include_str!("../assets/cytoscape.min.js");
// fcose (compound-aware layout) and its dependencies, loaded after Cytoscape.
const FCOSE: [&str; 3] = [
    include_str!("../assets/layout-base.js"),
    include_str!("../assets/cose-base.js"),
    include_str!("../assets/cytoscape-fcose.js"),
];
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

pub fn data(run: &Run, root: &Path, opts: &Opts) -> Value {
    let base_sha = run.result.base.clone();
    let head_ref = opts.head.as_ref().and(run.result.head.clone());
    let mut after_files: HashMap<String, String> = HashMap::new();
    let mut before_files: HashMap<String, String> = HashMap::new();
    let mut after = |file: &str| -> String {
        after_files
            .entry(file.to_string())
            .or_insert_with(|| match (&head_ref, &run.repo) {
                (Some(h), Some(r)) => r.show(h, file).unwrap_or_default(),
                _ => std::fs::read_to_string(root.join(file)).unwrap_or_default(),
            })
            .clone()
    };
    let mut before = |file: &str| -> String {
        before_files
            .entry(file.to_string())
            .or_insert_with(|| match (&base_sha, &run.repo) {
                (Some(b), Some(r)) => r.show(b, file).unwrap_or_default(),
                _ => String::new(),
            })
            .clone()
    };
    let layers = checks::Layers::new(&run.config, &run.packages);
    let bp = &run.config.blueprint;
    let set = |g: &[String]| workspace::globset(g).unwrap_or_else(|_| globset::GlobSet::empty());
    let lane_globs: Vec<(globset::GlobSet, globset::GlobSet)> = bp
        .lanes
        .iter()
        .map(|l| (set(&l.matches), set(&l.exclude)))
        .collect();
    let test_files = set(&run.config.tests);
    // Blueprint lane: the first lane whose globs or layers match; systems go in a last "Systems" lane.
    let lane_of = |n: &Node| -> Option<usize> {
        if n.kind == "system" {
            return Some(bp.lanes.len());
        }
        if test_files.is_match(&n.file) {
            return None;
        }
        let layer = layers.of(&n.file);
        bp.lanes.iter().enumerate().position(|(i, l)| {
            let (inc, exc) = &lane_globs[i];
            !exc.is_match(&n.file)
                && (inc.is_match(&n.file) || layer.is_some_and(|x| l.layers.iter().any(|y| y == x)))
        })
    };
    let base_nodes: HashMap<&str, &Node> = run
        .base_graph
        .iter()
        .flat_map(|b| b.nodes.iter())
        .map(|n| (n.id.as_str(), n))
        .collect();
    let nodes: Vec<Value> = all_nodes(run)
        .into_iter()
        .map(|(n, s)| {
            let mut v = json!({
                "id": n.id, "kind": n.kind, "name": n.name, "file": n.file, "line": n.range.start_line,
                "endLine": n.range.end_line, "parent": n.parent, "status": s, "complexity": n.metrics.complexity,
                "loc": n.metrics.loc, "intents": n.intents, "module": n.is_module(), "layer": layers.of(&n.file), "lane": lane_of(n), "test": test_files.is_match(&n.file),
            });
            if matches!(s, Status::Added | Status::Changed) && !n.is_module() {
                v["after"] = json!(slice(&after(&n.file), n.range.start_line, n.range.end_line));
            }
            if matches!(s, Status::Changed | Status::Removed) && !n.is_module()
                && let Some(b) = base_nodes.get(n.id.as_str()) {
                    v["before"] = json!(slice(&before(&b.file), b.range.start_line, b.range.end_line));
                }
            v
        })
        .collect();
    let viol = violations(run);
    let edges: Vec<Value> = all_edges(run)
        .into_iter()
        .map(|(e, s)| {
            json!({"from": e.from, "to": e.to, "kind": e.kind, "confidence": e.confidence, "status": s,
                   "violation": viol.contains(&(e.from.clone(), e.to.clone()))})
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
    let mode = if set.is_empty() || set.len() > 300 {
        "architecture"
    } else {
        "changed"
    };
    let layer_names: Vec<&str> = run.config.layers.iter().map(|(n, _)| n.as_str()).collect();
    json!({
        "nodes": nodes, "edges": edges, "findings": findings, "intents": intents,
        "intentChanges": run.result.intent_changes, "notices": run.result.notices,
        "editor": run.config.editor, "root": root.to_string_lossy(), "mode": mode, "layers": layer_names,
        "lanes": if bp.lanes.is_empty() { vec![] } else { bp.lanes.iter().map(|l| l.name.as_str()).chain(["Systems"]).collect::<Vec<_>>() },
        "blocking": run.result.blocking, "base": run.result.base, "head": run.result.head, "counts": run.result.counts,
    })
}

pub fn render(run: &Run, root: &Path, opts: &Opts, live: bool) -> String {
    let d = data(run, root, opts).to_string().replace('<', "\\u003c");
    page(&d, live)
}

fn page(data_json: &str, live: bool) -> String {
    TEMPLATE
        .replace(
            "/*IG_CYTOSCAPE*/",
            &[CYTOSCAPE, FCOSE[0], FCOSE[1], FCOSE[2]].join("\n;\n"),
        )
        .replace("/*IG_SCRIPT*/", SCRIPT)
        .replace("/*IG_LIVE*/", if live { "true" } else { "false" })
        .replace("/*IG_DATA*/", data_json)
}

pub fn write(root: &Path, opts: &Opts) -> Result<PathBuf> {
    let run = check::run(root, opts)?;
    let html = render(&run, root, opts, false);
    let path = root.join(".ig/view.html");
    check::write_atomic(&path, html.as_bytes())?;
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

pub fn watch(root: &Path, opts: &Opts, open_browser: bool) -> Result<()> {
    use notify::Watcher;
    let opts = opts.clone();
    let build = move |root: &Path| -> Result<String> {
        let opts = &opts;
        let run = check::run(root, opts)?;
        let html = render(&run, root, opts, true);
        let _ = check::write_atomic(
            &root.join(".ig/view.html"),
            render(&run, root, opts, false).as_bytes(),
        );
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
