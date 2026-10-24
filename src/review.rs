//! `ig review`: module graph, structural rules, ranking, and the structural regressions `check` uses.

use crate::checks::Layers;
use crate::config::{Config, ReviewConfig, Severity};
use crate::model::{Finding, Graph, Node, sha1_hex};
use crate::workspace::Package;
use globset::GlobSet;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;

pub struct Ctx<'a> {
    pub config: &'a Config,
    pub tests: &'a GlobSet,
    pub packages: &'a [Package],
    pub entry_points: &'a GlobSet,
}

/// Module-level view of a graph.
pub struct ModuleGraph<'g> {
    pub graph: &'g Graph,
    pub modules: Vec<&'g str>,
    pub out: BTreeMap<&'g str, BTreeSet<&'g str>>,
    pub inn: BTreeMap<&'g str, BTreeSet<&'g str>>,
    pub use_out: BTreeMap<&'g str, BTreeSet<&'g str>>,
    pub use_in: BTreeMap<&'g str, BTreeSet<&'g str>>,
    pub nodes: HashMap<&'g str, &'g Node>,
    pub dir: HashMap<&'g str, String>,
    pub source_roots: HashSet<String>,
}

const USES: [&str; 4] = ["calls", "renders", "extends", "implements"];

impl<'g> ModuleGraph<'g> {
    pub fn new(graph: &'g Graph, ctx: &Ctx) -> ModuleGraph<'g> {
        let nodes: HashMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let modules: Vec<&str> =
            graph.nodes.iter().filter(|n| n.is_module() && !ctx.tests.is_match(&n.file)).map(|n| n.id.as_str()).collect();
        let mset: HashSet<&str> = modules.iter().copied().collect();
        let mut mg = ModuleGraph {
            graph,
            modules: modules.clone(),
            out: BTreeMap::new(),
            inn: BTreeMap::new(),
            use_out: BTreeMap::new(),
            use_in: BTreeMap::new(),
            nodes,
            dir: HashMap::new(),
            source_roots: HashSet::new(),
        };
        for m in &modules {
            mg.out.insert(m, BTreeSet::new());
            mg.inn.insert(m, BTreeSet::new());
            mg.use_out.insert(m, BTreeSet::new());
            mg.use_in.insert(m, BTreeSet::new());
        }
        for e in graph.edges.iter().filter(|e| e.resolved()) {
            let (Some(f), Some(t)) = (mg.nodes.get(e.from.as_str()), mg.nodes.get(e.to.as_str())) else { continue };
            let (f, t) = (f.file.as_str(), t.file.as_str());
            if f == t || !mset.contains(f) || !mset.contains(t) {
                continue;
            }
            mg.out.get_mut(f).unwrap().insert(t);
            mg.inn.get_mut(t).unwrap().insert(f);
            if USES.contains(&e.kind.as_str()) {
                mg.use_out.get_mut(f).unwrap().insert(t);
                mg.use_in.get_mut(t).unwrap().insert(f);
            }
        }
        // Source roots: the package dir, or its src/ when the package has modules there.
        let mut pkg_root: Vec<(String, String)> = vec![];
        for p in ctx.packages {
            let src = crate::workspace::join(&p.dir, "src");
            let has_src = modules.iter().any(|m| m.starts_with(&format!("{src}/")));
            let root = if has_src { src } else { p.dir.clone() };
            mg.source_roots.insert(root.clone());
            pkg_root.push((p.dir.clone(), root));
        }
        pkg_root.sort_by_key(|(d, _)| std::cmp::Reverse(d.len()));
        for m in &modules {
            let root = pkg_root
                .iter()
                .find(|(d, _)| d == "." || m.starts_with(&format!("{d}/")))
                .map(|(_, r)| r.clone())
                .unwrap_or_else(|| ".".into());
            let rest = if root == "." { m.to_string() } else { m.strip_prefix(&format!("{root}/")).unwrap_or(m).to_string() };
            let d = match rest.split_once('/') {
                Some((seg, _)) => crate::workspace::join(&root, seg),
                None => root.clone(),
            };
            mg.dir.insert(m, d);
        }
        mg
    }

    pub fn fan_in(&self, m: &str) -> usize {
        self.inn.get(m).map_or(0, |s| s.len())
    }

    /// Strongly connected components of size ≥ 2 over `out` (iterative Tarjan), each sorted.
    pub fn sccs(&self) -> Vec<Vec<&'g str>> {
        let idx: HashMap<&str, usize> = self.modules.iter().enumerate().map(|(i, m)| (*m, i)).collect();
        let adj: Vec<Vec<usize>> = self.modules.iter().map(|m| self.out[m].iter().map(|t| idx[t]).collect()).collect();
        tarjan(&adj)
            .into_iter()
            .filter(|c| c.len() >= 2)
            .map(|c| {
                let mut v: Vec<&str> = c.iter().map(|i| self.modules[*i]).collect();
                v.sort();
                v
            })
            .collect()
    }

    /// Shortest loop from `start` back to itself inside `within`.
    pub fn shortest_loop(&self, start: &'g str, within: &HashSet<&str>) -> Vec<&'g str> {
        let mut prev: HashMap<&str, &str> = HashMap::new();
        let mut q = VecDeque::from([start]);
        while let Some(cur) = q.pop_front() {
            for &t in &self.out[cur] {
                if !within.contains(t) {
                    continue;
                }
                if t == start {
                    let mut path = vec![cur];
                    let mut c = cur;
                    while c != start {
                        c = prev[c];
                        path.push(c);
                    }
                    path.reverse();
                    path.push(start);
                    return path;
                }
                if !prev.contains_key(t) {
                    prev.insert(t, cur);
                    q.push_back(t);
                }
            }
        }
        vec![start]
    }

    /// Hub thresholds (fan-in, fan-out) over active modules.
    pub fn hub_thresholds(&self, r: &ReviewConfig) -> (usize, usize) {
        let active: Vec<&str> =
            self.modules.iter().copied().filter(|m| !self.use_in[m].is_empty() || !self.use_out[m].is_empty()).collect();
        let pct = |mut v: Vec<usize>| -> usize {
            if v.is_empty() {
                return r.hub_min;
            }
            v.sort();
            let i = ((r.hub_percentile * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1;
            v[i].max(r.hub_min)
        };
        (pct(active.iter().map(|m| self.use_in[m].len()).collect()), pct(active.iter().map(|m| self.use_out[m].len()).collect()))
    }

    pub fn hubs(&self, th: (usize, usize)) -> Vec<&'g str> {
        self.modules.iter().copied().filter(|m| self.use_in[m].len() >= th.0 && self.use_out[m].len() >= th.1).collect()
    }

    fn top_level(&self, id: &'g str) -> Option<&'g str> {
        let mut cur = self.nodes.get(id)?;
        loop {
            let p = cur.parent.as_deref()?;
            let pn = self.nodes.get(p)?;
            if pn.is_module() {
                return Some(cur.id.as_str());
            }
            cur = pn;
        }
    }

    /// Cohesion: (members, jobs) of module `m`.
    pub fn cohesion(&self, m: &str, ctx: &Ctx) -> (usize, usize) {
        let members: Vec<&str> = self
            .graph
            .nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(m) && n.kind != "test")
            .map(|n| n.id.as_str())
            .collect();
        if members.len() < 2 {
            return (members.len(), 0);
        }
        let ix: HashMap<&str, usize> = members.iter().enumerate().map(|(i, x)| (*x, i)).collect();
        let mut uf: Vec<usize> = (0..members.len()).collect();
        fn find(uf: &mut Vec<usize>, x: usize) -> usize {
            let mut r = x;
            while uf[r] != r {
                r = uf[r];
            }
            let mut c = x;
            while uf[c] != r {
                let n = uf[c];
                uf[c] = r;
                c = n;
            }
            r
        }
        let mut callers: HashMap<&str, usize> = HashMap::new();
        for e in &self.graph.edges {
            if e.kind == "imports" || e.from == m {
                continue;
            }
            let Some(to) = self.top_level(&e.to).filter(|t| ix.contains_key(t)) else { continue };
            let Some(from_node) = self.nodes.get(e.from.as_str()) else { continue };
            if from_node.file == m {
                if let Some(f) = self.top_level(&e.from).filter(|f| ix.contains_key(f)) {
                    let (a, b) = (find(&mut uf, ix[f]), find(&mut uf, ix[to]));
                    uf[a] = b;
                }
            } else if !ctx.tests.is_match(&from_node.file) && from_node.kind != "test" {
                let caller = e.from.as_str();
                match callers.get(caller) {
                    Some(&prev) => {
                        let (a, b) = (find(&mut uf, prev), find(&mut uf, ix[to]));
                        uf[a] = b;
                    }
                    None => {
                        callers.insert(caller, ix[to]);
                    }
                }
            }
        }
        let mut sizes: HashMap<usize, usize> = HashMap::new();
        for i in 0..members.len() {
            *sizes.entry(find(&mut uf, i)).or_insert(0) += 1;
        }
        (members.len(), sizes.values().filter(|s| **s >= 2).count())
    }
}

fn tarjan(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adj.len();
    let (mut index, mut low, mut on) = (vec![usize::MAX; n], vec![0; n], vec![false; n]);
    let (mut stack, mut out, mut next) = (vec![], vec![], 0);
    for s in 0..n {
        if index[s] != usize::MAX {
            continue;
        }
        let mut work = vec![(s, 0usize)];
        while let Some(&mut (v, ref mut ei)) = work.last_mut() {
            if *ei == 0 && index[v] == usize::MAX {
                index[v] = next;
                low[v] = next;
                next += 1;
                stack.push(v);
                on[v] = true;
            }
            if *ei < adj[v].len() {
                let w = adj[v][*ei];
                *ei += 1;
                if index[w] == usize::MAX {
                    work.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            work.pop();
            if let Some(&(p, _)) = work.last() {
                low[p] = low[p].min(low[v]);
            }
            if low[v] == index[v] {
                let mut c = vec![];
                loop {
                    let w = stack.pop().unwrap();
                    on[w] = false;
                    c.push(w);
                    if w == v {
                        break;
                    }
                }
                out.push(c);
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub rule_id: String,
    pub title: String,
    pub evidence: String,
    pub why: String,
    pub fix: String,
    pub modules: Vec<String>,
    pub score: usize,
    pub fingerprint: String,
    #[serde(skip)]
    fan_in: usize,
}

fn weight(rule: &str) -> usize {
    match rule {
        "struct/cycle" | "struct/hub" => 3,
        "struct/unstable-dependency" | "struct/low-cohesion" | "struct/change-coupling" => 2,
        _ => 1,
    }
}

pub fn fingerprint(rule: &str, modules: &[String]) -> String {
    let mut m = modules.to_vec();
    m.sort();
    m.dedup();
    sha1_hex(&format!("{rule}|{}", m.join(",")))
}

fn item(mg: &ModuleGraph, rule: &str, affected: usize, modules: Vec<String>, title: String, evidence: String, why: &str, fix: &str) -> ReviewItem {
    let mut modules = modules;
    modules.sort();
    modules.dedup();
    let fan_in = modules.first().map_or(0, |m| mg.fan_in(m));
    ReviewItem {
        rule_id: rule.into(),
        fingerprint: fingerprint(rule, &modules),
        score: affected * weight(rule),
        title,
        evidence,
        why: why.into(),
        fix: fix.into(),
        modules,
        fan_in,
    }
}

fn chain(path: &[&str]) -> String {
    path.join(" → ")
}

pub fn review(graph: &Graph, ctx: &Ctx, root: &Path) -> Vec<ReviewItem> {
    let mg = ModuleGraph::new(graph, ctx);
    let r = &ctx.config.review;
    let mut items = vec![];

    // Cycles.
    for c in mg.sccs() {
        let within: HashSet<&str> = c.iter().copied().collect();
        let lp = mg.shortest_loop(c[0], &within);
        items.push(item(
            &mg,
            "struct/cycle",
            c.len(),
            c.iter().map(|s| s.to_string()).collect(),
            format!("Import cycle between {} modules", c.len()),
            chain(&lp),
            "Modules in a cycle can't be understood, tested or changed one at a time.",
            "Move the shared piece into its own module, or invert one dependency.",
        ));
    }
    if !ctx.config.layers.is_empty() {
        items.extend(layer_cycles(&mg, ctx));
    }

    // Hubs.
    let th = mg.hub_thresholds(r);
    for h in mg.hubs(th) {
        let (i, o) = (mg.use_in[h].len(), mg.use_out[h].len());
        items.push(item(
            &mg,
            "struct/hub",
            i + o,
            vec![h.to_string()],
            format!("{h} is a hub"),
            format!("used by {i} modules and uses {o} (thresholds {} / {})", th.0, th.1),
            "A module that many depend on and that depends on many spreads every change.",
            "Split it by job, so callers depend only on the part they use.",
        ));
    }

    // Unstable dependencies, over dir-level module edges.
    let mut dir_in: HashMap<&str, usize> = HashMap::new();
    let mut dir_out: HashMap<&str, usize> = HashMap::new();
    let mut dir_edges: BTreeMap<(&str, &str), Vec<(&str, &str)>> = BTreeMap::new();
    for (f, ts) in &mg.out {
        for t in ts {
            let (a, b) = (mg.dir[f].as_str(), mg.dir[t].as_str());
            if a != b {
                *dir_out.entry(a).or_default() += 1;
                *dir_in.entry(b).or_default() += 1;
                dir_edges.entry((a, b)).or_default().push((f, t));
            }
        }
    }
    let inst = |d: &str| -> f64 {
        let (i, o) = (*dir_in.get(d).unwrap_or(&0) as f64, *dir_out.get(d).unwrap_or(&0) as f64);
        if i + o == 0.0 { 0.0 } else { o / (i + o) }
    };
    for ((a, b), es) in &dir_edges {
        if *dir_in.get(a).unwrap_or(&0) < r.stable_fan_in || mg.source_roots.contains(*b) {
            continue;
        }
        if inst(b) >= inst(a) + r.instability_gap {
            let mods: Vec<String> = es.iter().flat_map(|(f, t)| [f.to_string(), t.to_string()]).collect();
            items.push(item(
                &mg,
                "struct/unstable-dependency",
                es.len(),
                mods,
                format!("Stable {a} depends on unstable {b}"),
                format!(
                    "instability {:.2} → {:.2}; e.g. {} → {}",
                    inst(a),
                    inst(b),
                    es[0].0,
                    es[0].1
                ),
                "Code many others rely on should not depend on code that changes often.",
                "Depend on an interface owned by the stable side, or move the used part down.",
            ));
        }
    }

    // Low cohesion.
    for m in &mg.modules {
        let (size, jobs) = mg.cohesion(m, ctx);
        if jobs >= r.cohesion_groups && size >= r.cohesion_min_members {
            items.push(item(
                &mg,
                "struct/low-cohesion",
                jobs,
                vec![m.to_string()],
                format!("{m} does {jobs} unrelated jobs"),
                format!("{size} top-level members fall into {jobs} groups that never call each other or share callers"),
                "Unrelated jobs in one module change for unrelated reasons.",
                "Split the module along those groups.",
            ));
        }
    }

    // Change coupling.
    items.extend(change_coupling(&mg, r, root));

    // Unused exports.
    items.extend(unused_exports(&mg, ctx));

    // Oversized.
    for m in &mg.modules {
        let n = mg.nodes[m];
        let big: Vec<&Node> = graph
            .nodes
            .iter()
            .filter(|x| x.file == n.file && !x.is_module() && x.metrics.complexity as usize >= r.oversized_complexity)
            .collect();
        if n.metrics.loc as usize >= r.oversized_loc || !big.is_empty() {
            let mut ev = vec![];
            if n.metrics.loc as usize >= r.oversized_loc {
                ev.push(format!("{} lines", n.metrics.loc));
            }
            for b in big.iter().take(3) {
                ev.push(format!("{} has complexity {}", b.name, b.metrics.complexity));
            }
            items.push(item(
                &mg,
                "struct/oversized",
                1 + big.len(),
                vec![m.to_string()],
                format!("{m} is oversized"),
                ev.join("; "),
                "Large modules and very complex functions are hard to review and easy to break.",
                "Extract cohesive parts into their own modules or functions.",
            ));
        }
    }

    items.sort_by(|a, b| b.score.cmp(&a.score).then(b.fan_in.cmp(&a.fan_in)).then(a.fingerprint.cmp(&b.fingerprint)));
    items
}

/// The top `top` items with at most 3 per rule, then the rest in score order.
pub fn rank(items: Vec<ReviewItem>, top: usize) -> (Vec<ReviewItem>, Vec<ReviewItem>) {
    let mut head = vec![];
    let mut rest = vec![];
    let mut per_rule: HashMap<String, usize> = HashMap::new();
    for it in items {
        let c = per_rule.entry(it.rule_id.clone()).or_insert(0);
        if head.len() < top && *c < 3 {
            *c += 1;
            head.push(it);
        } else {
            rest.push(it);
        }
    }
    (head, rest)
}

fn layer_cycles(mg: &ModuleGraph, ctx: &Ctx) -> Vec<ReviewItem> {
    let layers = Layers::new(ctx.config, ctx.packages);
    let names: Vec<&str> = ctx.config.layers.iter().map(|(n, _)| n.as_str()).collect();
    let li: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut edges: BTreeMap<(usize, usize), Vec<(&str, &str)>> = BTreeMap::new();
    for (f, ts) in &mg.out {
        let Some(a) = layers.of(f) else { continue };
        for t in ts {
            let Some(b) = layers.of(t) else { continue };
            if a != b {
                edges.entry((li[a], li[b])).or_default().push((f, t));
            }
        }
    }
    let mut adj = vec![vec![]; names.len()];
    for (a, b) in edges.keys() {
        adj[*a].push(*b);
    }
    let mut out = vec![];
    for comp in tarjan(&adj).into_iter().filter(|c| c.len() >= 2) {
        let set: HashSet<usize> = comp.iter().copied().collect();
        let dirs: Vec<(usize, usize)> = edges.keys().copied().filter(|(a, b)| set.contains(a) && set.contains(b)).collect();
        let cnt = |d: &(usize, usize)| edges[d].len();
        // Back edges: in each two-way pair the cheaper direction (tie: later-declared source layer).
        let mut back: Vec<(usize, usize)> = vec![];
        for &(a, b) in &dirs {
            if dirs.contains(&(b, a)) {
                let (ca, cb) = (cnt(&(a, b)), cnt(&(b, a)));
                if ca < cb || (ca == cb && a > b) {
                    back.push((a, b));
                }
            }
        }
        if back.is_empty() {
            if let Some(w) = dirs.iter().min_by_key(|d| (cnt(d), std::cmp::Reverse(d.0))) {
                back.push(*w);
            }
        }
        let mut sorted = dirs.clone();
        sorted.sort_by_key(|d| (!back.contains(d), cnt(d), *d));
        let shown: Vec<(usize, usize)> = sorted.into_iter().take(3).collect();
        let mut modules = vec![];
        let ev: Vec<String> = shown
            .iter()
            .map(|d| {
                let ex: Vec<String> = edges[d].iter().take(3).map(|(f, t)| {
                    modules.push(f.to_string());
                    modules.push(t.to_string());
                    format!("{f} → {t}")
                }).collect();
                format!("{} → {} ({} edges{}): {}", names[d.0], names[d.1], cnt(d), if back.contains(d) { ", back edge" } else { "" }, ex.join(", "))
            })
            .collect();
        let mut lnames: Vec<&str> = comp.iter().map(|i| names[*i]).collect();
        lnames.sort();
        let it = item(
            mg,
            "struct/cycle",
            comp.len(),
            modules,
            format!("Layer cycle between {}", lnames.join(", ")),
            ev.join("; "),
            "Layers that depend on each other are no longer layers.",
            "Remove the back edges listed first; they are the cheapest to cut.",
        );
        out.push(it);
    }
    out
}

fn change_coupling(mg: &ModuleGraph, r: &ReviewConfig, root: &Path) -> Vec<ReviewItem> {
    let n = r.history_commits.to_string();
    let Ok(log) = crate::git::git(root, &["log", "--relative", "--name-only", "--format=%x00", "-n", &n]) else { return vec![] };
    let mods: HashSet<&str> = mg.modules.iter().copied().collect();
    let mut changes: HashMap<&str, usize> = HashMap::new();
    let mut pairs: HashMap<(&str, &str), usize> = HashMap::new();
    for commit in log.split('\0') {
        let files: Vec<&str> = commit.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if files.len() > 50 {
            continue;
        }
        let mut fs: Vec<&str> = files.iter().filter_map(|f| mods.get(f).copied()).collect();
        fs.sort();
        fs.dedup();
        for f in &fs {
            *changes.entry(f).or_default() += 1;
        }
        for i in 0..fs.len() {
            for j in i + 1..fs.len() {
                *pairs.entry((fs[i], fs[j])).or_default() += 1;
            }
        }
    }
    let mut out = vec![];
    let mut keys: Vec<_> = pairs.into_iter().collect();
    keys.sort();
    for ((a, b), c) in keys {
        let rarer = changes[a].min(changes[b]);
        if c < r.coupling_min || (c as f64) < r.coupling_ratio * rarer as f64 {
            continue;
        }
        if mg.dir[a] == mg.dir[b] || mg.out[a].contains(b) || mg.out[b].contains(a) {
            continue;
        }
        out.push(item(
            mg,
            "struct/change-coupling",
            2,
            vec![a.to_string(), b.to_string()],
            format!("{a} and {b} change together"),
            format!("{c} shared commits out of {rarer}, with no dependency between them"),
            "Files that always change together but don't reference each other hide a dependency.",
            "Make the shared concept explicit in one module both depend on.",
        ));
    }
    out
}

fn unused_exports(mg: &ModuleGraph, ctx: &Ctx) -> Vec<ReviewItem> {
    let g = mg.graph;
    // Skip package entries and everything they re-export (transitively), and entryPoints.
    let mut skip: HashSet<&str> = HashSet::new();
    let mut q: Vec<&str> = ctx.packages.iter().filter_map(|p| p.source_entry.as_deref()).collect();
    q.extend(mg.modules.iter().copied().filter(|m| ctx.entry_points.is_match(m)));
    while let Some(m) = q.pop() {
        if !skip.insert(m) {
            continue;
        }
        for e in g.edges.iter().filter(|e| e.from == m && e.kind == "imports" && e.reexport) {
            q.push(&e.to);
        }
    }
    let ns_imported: HashSet<&str> = g.edges.iter().filter(|e| e.kind == "imports" && e.namespace).map(|e| e.to.as_str()).collect();
    let is_test = |id: &str| mg.nodes.get(id).is_some_and(|n| n.kind == "test" || ctx.tests.is_match(&n.file));
    let mut used: HashSet<&str> = HashSet::new();
    let mut test_used: HashSet<&str> = HashSet::new();
    for e in g.edges.iter().filter(|e| USES.contains(&e.kind.as_str())) {
        let Some(to) = mg.top_level(&e.to) else { continue };
        let (Some(fnode), Some(tnode)) = (mg.nodes.get(e.from.as_str()), mg.nodes.get(to)) else { continue };
        if fnode.file == tnode.file {
            continue;
        }
        if is_test(&e.from) {
            test_used.insert(to);
        } else {
            used.insert(to);
        }
    }
    let mut out = vec![];
    for m in &mg.modules {
        if skip.contains(m) || ns_imported.contains(m) {
            continue;
        }
        let unused: Vec<&Node> = g
            .nodes
            .iter()
            .filter(|n| n.parent.as_deref() == Some(m) && n.exported && !used.contains(n.id.as_str()))
            .collect();
        if unused.is_empty() {
            continue;
        }
        let names: Vec<&str> = unused.iter().map(|n| n.name.as_str()).collect();
        let tests_only: Vec<&str> = unused.iter().filter(|n| test_used.contains(n.id.as_str())).map(|n| n.name.as_str()).collect();
        let mut ev = format!("unused: {}", names.join(", "));
        if !tests_only.is_empty() {
            ev.push_str(&format!("; used only by tests: {}", tests_only.join(", ")));
        }
        out.push(item(
            mg,
            "struct/unused-export",
            unused.len(),
            vec![m.to_string()],
            format!("{m} exports {} unused symbol{}", unused.len(), if unused.len() == 1 { "" } else { "s" }),
            ev,
            "Exports nobody uses are dead weight and widen the module's surface.",
            "Remove them, or drop the export keyword if they are used locally.",
        ));
    }
    out
}

/// Structural regressions against the base, as always-on warnings.
pub fn regressions(base: &Graph, head: &Graph, ctx: &Ctx) -> Vec<Finding> {
    let bg = ModuleGraph::new(base, ctx);
    let hg = ModuleGraph::new(head, ctx);
    let mut out = vec![];
    let mk = |rule: &str, mods: Vec<String>, msg: String| {
        let mut mods = mods;
        mods.sort();
        mods.dedup();
        let mut f = Finding::new(rule, Severity::Warning, "ig", &mods[0], 1, msg);
        f.fingerprint = fingerprint(rule, &mods);
        f
    };
    let base_sccs: Vec<HashSet<&str>> = bg.sccs().into_iter().map(|c| c.into_iter().collect()).collect();
    for c in hg.sccs() {
        if !base_sccs.iter().any(|b| c.iter().all(|m| b.contains(m))) {
            let within: HashSet<&str> = c.iter().copied().collect();
            let lp = hg.shortest_loop(c[0], &within);
            out.push(mk("struct/new-cycle", c.iter().map(|s| s.to_string()).collect(), format!("new import cycle: {}", chain(&lp))));
        }
    }
    let r = &ctx.config.review;
    let th = bg.hub_thresholds(r);
    let base_hubs: HashSet<&str> = bg.hubs(th).into_iter().collect();
    for h in hg.hubs(th) {
        if !base_hubs.contains(h) {
            out.push(mk(
                "struct/new-hub",
                vec![h.to_string()],
                format!("{h} became a hub: used by {} modules, uses {}", hg.use_in[h].len(), hg.use_out[h].len()),
            ));
        }
    }
    let base_mods: HashSet<&str> = bg.modules.iter().copied().collect();
    for m in &hg.modules {
        let (size, jobs) = hg.cohesion(m, ctx);
        let drop = if base_mods.contains(m) {
            let (_, before) = bg.cohesion(m, ctx);
            jobs >= before + 2
        } else {
            jobs >= r.cohesion_groups && size >= r.cohesion_min_members
        };
        if drop {
            out.push(mk("struct/cohesion-drop", vec![m.to_string()], format!("{m} now does {jobs} unrelated jobs")));
        }
    }
    out
}

pub fn print(head: &[ReviewItem], rest: usize, all: bool, rest_items: &[ReviewItem]) {
    let show = |i: usize, it: &ReviewItem| {
        println!("{}. {} [{}]", i + 1, it.title, it.rule_id);
        println!("   {}", it.evidence);
        println!("   why: {}", it.why);
        println!("   fix: {}", it.fix);
    };
    if head.is_empty() {
        println!("ig review: no structural problems found");
        return;
    }
    for (i, it) in head.iter().enumerate() {
        show(i, it);
    }
    if all {
        for (i, it) in rest_items.iter().enumerate() {
            show(head.len() + i, it);
        }
    } else if rest > 0 {
        println!("\n{rest} more — ig review --all");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tarjan_finds_cycles() {
        let adj = vec![vec![1], vec![2], vec![0], vec![0]];
        let c: Vec<Vec<usize>> = tarjan(&adj).into_iter().filter(|c| c.len() > 1).collect();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].len(), 3);
    }

    #[test]
    fn rank_caps_per_rule() {
        let mk = |rule: &str, score| ReviewItem {
            rule_id: rule.into(),
            title: String::new(),
            evidence: String::new(),
            why: String::new(),
            fix: String::new(),
            modules: vec![],
            score,
            fingerprint: String::new(),
            fan_in: 0,
        };
        let items = vec![mk("a", 9), mk("a", 8), mk("a", 7), mk("a", 6), mk("b", 1)];
        let (head, rest) = rank(items, 10);
        assert_eq!(head.len(), 4);
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].score, 6);
    }
}
