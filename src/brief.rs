//! `ig brief`: a change read as architecture and per-function code diffs, in architecture order.

use crate::check::Run;
use crate::checks::Layers;
use crate::config::Severity;
use crate::diff::Status;
use crate::model::{Edge, Node};
use crate::view::LaneMap;
use std::collections::{BTreeMap, HashMap, HashSet};

const MAX_FUNCTIONS: usize = 40;
const MAX_SEQUENCES: usize = 5;
const MAX_MESSAGES: usize = 60;

fn mark(s: Status) -> &'static str {
    match s {
        Status::Added => "+",
        Status::Changed => "~",
        Status::Removed => "-",
        Status::Unchanged => " ",
    }
}

fn stem(file: &str) -> String {
    let b = file.rsplit('/').next().unwrap_or(file);
    b.split('.').next().unwrap_or(b).to_string()
}

struct Graph<'r> {
    nodes: HashMap<&'r str, &'r Node>,
    out: HashMap<&'r str, Vec<&'r Edge>>,
}

impl<'r> Graph<'r> {
    fn top_member(&self, n: &'r Node) -> &'r Node {
        let mut cur = n;
        while let Some(p) = cur.parent.as_deref().and_then(|p| self.nodes.get(p)) {
            if p.is_module() {
                break;
            }
            cur = p;
        }
        cur
    }

    /// (participant key, label, is a system)
    fn participant(&self, id: &str) -> (String, String, bool) {
        let Some(n) = self.nodes.get(id) else {
            return (id.into(), id.into(), false);
        };
        if n.kind == "system" {
            return (n.id.clone(), n.name.clone(), true);
        }
        if n.kind == "route" {
            return (n.file.clone(), stem(&n.file), false);
        }
        let t = if n.is_module() { n } else { self.top_member(n) };
        if !n.is_module() && t.kind == "class" {
            (t.id.clone(), t.name.clone(), false)
        } else {
            (n.file.clone(), stem(&n.file), false)
        }
    }
}

/// Participants and (from, to, label, is_http) messages from `start`, in call order.
fn sequence(g: &Graph, start: &str) -> (Vec<(String, bool)>, Vec<(usize, usize, String, bool)>) {
    let mut parts: Vec<(String, String, bool)> = vec![];
    let mut msgs = vec![];
    let mut seen = HashSet::new();
    let mut uses_done = HashSet::new();
    fn part(g: &Graph, parts: &mut Vec<(String, String, bool)>, id: &str) -> usize {
        let (k, l, s) = g.participant(id);
        match parts.iter().position(|p| p.0 == k) {
            Some(i) => i,
            None => {
                parts.push((k, l, s));
                parts.len() - 1
            }
        }
    }
    fn walk<'r>(
        g: &Graph<'r>,
        id: &'r str,
        depth: usize,
        parts: &mut Vec<(String, String, bool)>,
        msgs: &mut Vec<(usize, usize, String, bool)>,
        seen: &mut HashSet<&'r str>,
        uses_done: &mut HashSet<&'r str>,
    ) {
        if !seen.insert(id) || msgs.len() >= MAX_MESSAGES {
            return;
        }
        let Some(n) = g.nodes.get(id) else { return };
        let from = part(g, parts, id);
        let keep = |e: &&&Edge| {
            g.nodes.get(e.to.as_str()).is_some_and(|t| {
                matches!(e.kind.as_str(), "calls" | "dispatch" | "http" | "uses")
                    && e.confidence != "guessed"
                    && !(t.kind == "class"
                        && (t.name.ends_with("Error") || t.name.ends_with("Exception")))
            })
        };
        let mut out: Vec<&Edge> = g
            .out
            .get(id)
            .map(|v| v.iter().filter(keep).copied().collect())
            .unwrap_or_default();
        if !n.is_module() && uses_done.insert(n.file.as_str()) {
            out.extend(
                g.out
                    .get(n.file.as_str())
                    .into_iter()
                    .flatten()
                    .filter(|e| e.kind == "uses"),
            );
        }
        out.sort_by_key(|e| e.line);
        for e in out {
            if msgs.len() >= MAX_MESSAGES {
                return;
            }
            let t = g.nodes[e.to.as_str()];
            let to = part(g, parts, &e.to);
            let label = if e.kind == "uses" {
                "uses".to_string()
            } else {
                t.name.clone()
            };
            msgs.push((from, to, label, e.kind == "http"));
            if depth < 4 && t.kind != "system" {
                walk(g, &e.to, depth + 1, parts, msgs, seen, uses_done);
            }
        }
    }
    part(g, &mut parts, start);
    walk(
        g,
        start,
        0,
        &mut parts,
        &mut msgs,
        &mut seen,
        &mut uses_done,
    );
    (parts.into_iter().map(|(_, l, s)| (l, s)).collect(), msgs)
}

fn clean(s: &str) -> String {
    s.replace([';', '#', '"'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn mermaid_sequence(parts: &[(String, bool)], msgs: &[(usize, usize, String, bool)]) -> String {
    let mut out = vec!["sequenceDiagram".to_string()];
    for (i, (l, _)) in parts.iter().enumerate() {
        out.push(format!("  participant p{i} as {}", clean(l)));
    }
    for (f, t, l, http) in msgs {
        out.push(format!(
            "  p{f}{}p{t}: {}",
            if *http { "-)" } else { "->>" },
            clean(l)
        ));
    }
    out.join("\n")
}

pub fn brief(run: &Run, root: &std::path::Path, opts: &crate::check::Opts, md: bool) -> String {
    let r = &run.result;
    let layers = Layers::new(&run.config, &run.packages);
    let lanes = LaneMap::new(&run.config, &layers);
    let use_lanes = !run.config.blueprint.lanes.is_empty();
    let mut all: Vec<&Node> = r.graph.nodes.iter().collect();
    if let Some(b) = &run.base_graph {
        all.extend(
            b.nodes
                .iter()
                .filter(|n| run.diff.status(&n.id) == Status::Removed),
        );
    }
    let g = Graph {
        nodes: all.iter().map(|n| (n.id.as_str(), *n)).collect(),
        out: {
            let mut m: HashMap<&str, Vec<&Edge>> = HashMap::new();
            for e in &r.graph.edges {
                m.entry(e.from.as_str()).or_default().push(e);
            }
            m
        },
    };
    let group_of = |n: &Node| -> (usize, String) {
        if use_lanes {
            let l = lanes.of(n);
            (l.unwrap_or(usize::MAX), lanes.name(l))
        } else {
            (
                0,
                layers
                    .of(&n.file)
                    .map(str::to_string)
                    .or_else(|| n.package.clone())
                    .unwrap_or_else(|| "other".into()),
            )
        }
    };

    let changed: Vec<(&Node, Status)> = all
        .iter()
        .filter_map(|n| {
            let s = run.diff.status(&n.id);
            (s != Status::Unchanged && !n.is_module() && n.kind != "test").then_some((*n, s))
        })
        .collect();

    let mut s = String::new();
    let h = |s: &mut String, level: usize, t: &str| {
        if md {
            s.push_str(&format!("{} {t}\n\n", "#".repeat(level)));
        } else {
            s.push_str(&format!(
                "{}\n{}\n",
                t.to_uppercase(),
                if level == 1 {
                    "=".repeat(t.len())
                } else {
                    String::new()
                }
            ));
        }
    };
    let base = r
        .base
        .as_deref()
        .map(|b| &b[..b.len().min(10)])
        .unwrap_or("(none)");
    let head = r
        .head
        .as_deref()
        .map(|b| &b[..b.len().min(10)])
        .unwrap_or("working tree");
    h(&mut s, 1, &format!("Change brief {base}..{head}"));
    let blocking = r
        .findings
        .iter()
        .filter(|f| f.severity >= run.config.block_on)
        .count();
    s.push_str(&format!(
        "{} added, {} changed, {} removed · {} findings ({} blocking)\n\n",
        r.counts.added,
        r.counts.changed,
        r.counts.removed,
        r.findings.len(),
        blocking
    ));
    if changed.is_empty() {
        s.push_str("No changed functions.\n");
        return s;
    }

    // 1. Architecture: what moved where.
    h(&mut s, 2, "Architecture");
    let mut groups: BTreeMap<(usize, String), BTreeMap<&str, Vec<(&Node, Status)>>> =
        BTreeMap::new();
    for (n, st) in &changed {
        groups
            .entry(group_of(n))
            .or_default()
            .entry(n.file.as_str())
            .or_default()
            .push((n, *st));
    }
    if md {
        s.push_str("```text\n");
    }
    for ((_, name), files) in &groups {
        s.push_str(&format!("{name}\n"));
        for (file, ns) in files {
            s.push_str(&format!("  {file}\n"));
            let mut ns = ns.clone();
            ns.sort_by_key(|(n, _)| n.range.start_line);
            for (n, st) in ns {
                s.push_str(&format!("    {} {} ({})\n", mark(st), n.name, n.kind));
            }
        }
    }
    if md {
        s.push_str("```\n");
    }
    s.push('\n');
    let added_edges: Vec<&Edge> = r
        .graph
        .edges
        .iter()
        .filter(|e| r.edge_changes.get(&e.key()) == Some(&Status::Added))
        .collect();
    let systems: BTreeMap<&str, usize> =
        added_edges
            .iter()
            .filter(|e| e.kind == "uses")
            .fold(BTreeMap::new(), |mut m, e| {
                *m.entry(
                    g.nodes
                        .get(e.to.as_str())
                        .map(|n| n.name.as_str())
                        .unwrap_or(&e.to),
                )
                .or_default() += 1;
                m
            });
    if !systems.is_empty() {
        let list: Vec<String> = systems.iter().map(|(k, v)| format!("{k} ({v})")).collect();
        s.push_str(&format!(
            "New dependencies on systems: {}\n\n",
            list.join(", ")
        ));
    }
    let mut links: BTreeMap<(String, String), usize> = BTreeMap::new();
    for e in added_edges.iter().filter(|e| {
        e.resolved() && matches!(e.kind.as_str(), "calls" | "dispatch" | "http" | "renders")
    }) {
        let (Some(a), Some(b)) = (g.nodes.get(e.from.as_str()), g.nodes.get(e.to.as_str())) else {
            continue;
        };
        let (ga, gb) = (group_of(a).1, group_of(b).1);
        if ga != gb {
            *links.entry((ga, gb)).or_default() += 1;
        }
    }
    if !links.is_empty() {
        let list: Vec<String> = links
            .iter()
            .map(|((a, b), n)| format!("{a} → {b} ({n})"))
            .collect();
        s.push_str(&format!(
            "New links between {}: {}\n\n",
            if use_lanes { "lanes" } else { "layers" },
            list.join(", ")
        ));
    }

    // 2. Code: each changed function as its own before/after diff, new functions in full, in lane
    // order. A node whose changed parts are shown on their own (methods, nested functions) is skipped.
    let changed_set: HashSet<&str> = changed.iter().map(|(n, _)| n.id.as_str()).collect();
    let mut has_changed_child: HashSet<&str> = HashSet::new();
    for (n, _) in &changed {
        let mut p = n.parent.as_deref();
        while let Some(id) = p {
            if !changed_set.contains(id) {
                break;
            }
            has_changed_child.insert(id);
            p = g.nodes.get(id).and_then(|x| x.parent.as_deref());
        }
    }
    let mut fns: Vec<&(&Node, Status)> = changed
        .iter()
        .filter(|(n, _)| !has_changed_child.contains(n.id.as_str()))
        .collect();
    fns.sort_by(|a, b| {
        (group_of(a.0).0, &a.0.file, a.0.range.start_line).cmp(&(
            group_of(b.0).0,
            &b.0.file,
            b.0.range.start_line,
        ))
    });
    if !fns.is_empty() {
        h(&mut s, 2, "Code");
        let mut sources = crate::view::Sources::new(run, root, opts);
        for (n, st) in fns.iter().take(MAX_FUNCTIONS) {
            let what = match st {
                Status::Added => "new",
                Status::Removed => "removed",
                _ => "changed",
            };
            let code = sources.code(n, *st);
            if md {
                s.push_str(&format!(
                    "**{} {}** ({what}) · `{}:{}`\n\n",
                    mark(*st),
                    n.name,
                    n.file,
                    n.range.start_line
                ));
            } else {
                s.push_str(&format!(
                    "{} {} ({what}) · {}:{}\n",
                    mark(*st),
                    n.name,
                    n.file,
                    n.range.start_line
                ));
            }
            let lines: Vec<(char, Option<u32>, String)> =
                match (&code.diff, &code.after, &code.before) {
                    (Some(d), _, _) => crate::linediff::hunks(d, 3)
                        .into_iter()
                        .map(|l| match l {
                            Some((op, old, new, t)) => (op.mark(), new.or(*old), t.clone()),
                            None => ('…', None, String::new()),
                        })
                        .collect(),
                    (None, Some((l0, t)), _) | (None, None, Some((l0, t))) => {
                        let m = if code.after.is_some() { '+' } else { '-' };
                        t.lines()
                            .enumerate()
                            .take(80)
                            .map(|(i, l)| (m, Some(l0 + i as u32), l.to_string()))
                            .collect()
                    }
                    _ => vec![],
                };
            if md {
                s.push_str("```diff\n");
                for (m, _, t) in &lines {
                    if *m == '…' {
                        s.push_str("@@ … @@\n");
                    } else {
                        s.push_str(&format!("{m}{t}\n"));
                    }
                }
                s.push_str("```\n\n");
            } else {
                for (m, l, t) in &lines {
                    match l {
                        Some(l) => s.push_str(&format!("  {m}{l:>5} │ {t}\n")),
                        None => s.push_str("        ┆\n"),
                    }
                }
                s.push('\n');
            }
        }
        if fns.len() > MAX_FUNCTIONS {
            s.push_str(&format!("…and {} more\n\n", fns.len() - MAX_FUNCTIONS));
        }
    }

    // 3. Sequences from the entry points of the change: changed nodes no other changed node calls.
    let changed_ids: HashSet<&str> = changed
        .iter()
        .filter(|(_, st)| *st != Status::Removed)
        .map(|(n, _)| n.id.as_str())
        .collect();
    let called: HashSet<&str> = r
        .graph
        .edges
        .iter()
        .filter(|e| changed_ids.contains(e.from.as_str()) && e.kind != "imports")
        .map(|e| e.to.as_str())
        .collect();
    let mut roots: Vec<&(&Node, Status)> = changed
        .iter()
        .filter(|(n, st)| {
            *st != Status::Removed && !called.contains(n.id.as_str()) && n.kind != "class"
        })
        .collect();
    roots.sort_by(|a, b| {
        (group_of(a.0).0, &a.0.file, a.0.range.start_line).cmp(&(
            group_of(b.0).0,
            &b.0.file,
            b.0.range.start_line,
        ))
    });
    let seqs: Vec<(
        String,
        (Vec<(String, bool)>, Vec<(usize, usize, String, bool)>),
    )> = roots
        .iter()
        .map(|(n, _)| (n.name.clone(), sequence(&g, &n.id)))
        .filter(|(_, (_, m))| m.len() >= 2)
        .take(MAX_SEQUENCES)
        .collect();
    if !seqs.is_empty() {
        h(&mut s, 2, "Sequences");
        for (name, (parts, msgs)) in seqs {
            if md {
                s.push_str(&format!(
                    "**{name}**\n\n```mermaid\n{}\n```\n\n",
                    mermaid_sequence(&parts, &msgs)
                ));
            } else {
                s.push_str(&format!("{name}\n"));
                for (f, t, l, http) in msgs {
                    s.push_str(&format!(
                        "    {} {} {}: {l}\n",
                        parts[f].0,
                        if http { "⇢" } else { "→" },
                        parts[t].0
                    ));
                }
                s.push('\n');
            }
        }
    }

    let blocking: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.severity >= Severity::Error)
        .collect();
    if !blocking.is_empty() {
        h(&mut s, 2, "Findings");
        for f in blocking.iter().take(20) {
            s.push_str(&format!(
                "- {}:{} [{}] {}\n",
                f.file, f.line, f.rule_id, f.message
            ));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brief_on_fixture() {
        let src =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
        let dir = std::env::temp_dir().join(format!("ig-brief-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            std::process::Command::new("cp")
                .arg("-R")
                .arg(&src)
                .arg(&dir)
                .status()
                .unwrap()
                .success()
        );
        // Outside git everything is added.
        let run = crate::check::run(&dir, &crate::check::Opts::default()).unwrap();
        let md = brief(&run, &dir, &crate::check::Opts::default(), true);
        assert!(md.contains("## Architecture"));
        assert!(md.contains("+ GET /users (route)"));
        assert!(md.contains(
            "**+ add** (new) · `packages/core/src/math.ts:2`\n\n```diff\n+export function add("
        ));
        // A class whose methods are shown on their own is not repeated.
        assert!(!md.contains("**+ Repo** (new)"));
        assert!(md.contains("```mermaid\nsequenceDiagram"));
        assert!(md.contains("New links between layers:") && md.contains("core → ui (1)"));
        let text = brief(&run, &dir, &crate::check::Opts::default(), false);
        assert!(text.starts_with("CHANGE BRIEF"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
