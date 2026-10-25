//! Module-level view of a graph: edges between files, SCCs, hubs, cohesion.

use super::Ctx;
use crate::config::ReviewConfig;
use crate::model::{Edge, Graph, Node};
use crate::workspace;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

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
    /// parent id → child nodes
    pub children: HashMap<&'g str, Vec<&'g Node>>,
    /// file → nodes
    pub by_file: HashMap<&'g str, Vec<&'g Node>>,
    /// file of `to` → non-import edges into it
    pub into_file: HashMap<&'g str, Vec<&'g Edge>>,
}

pub(super) const USES: [&str; 4] = ["calls", "renders", "extends", "implements"];

impl<'g> ModuleGraph<'g> {
    pub fn new(graph: &'g Graph, ctx: &Ctx) -> ModuleGraph<'g> {
        let nodes: HashMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let modules: Vec<&str> = graph
            .nodes
            .iter()
            .filter(|n| n.is_module() && !ctx.tests.is_match(&n.file))
            .map(|n| n.id.as_str())
            .collect();
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
            children: HashMap::new(),
            by_file: HashMap::new(),
            into_file: HashMap::new(),
        };
        for n in &graph.nodes {
            if let Some(p) = &n.parent {
                mg.children.entry(p.as_str()).or_default().push(n);
            }
            mg.by_file.entry(n.file.as_str()).or_default().push(n);
        }
        for e in graph.edges.iter().filter(|e| e.kind != "imports") {
            if let Some(t) = mg.nodes.get(e.to.as_str()) {
                mg.into_file.entry(t.file.as_str()).or_default().push(e);
            }
        }
        for m in &modules {
            mg.out.insert(m, BTreeSet::new());
            mg.inn.insert(m, BTreeSet::new());
            mg.use_out.insert(m, BTreeSet::new());
            mg.use_in.insert(m, BTreeSet::new());
        }
        for e in graph.edges.iter().filter(|e| e.resolved()) {
            let (Some(f), Some(t)) = (mg.nodes.get(e.from.as_str()), mg.nodes.get(e.to.as_str()))
            else {
                continue;
            };
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
            let src = workspace::join(&p.dir, "src");
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
            let rest = if root == "." {
                m.to_string()
            } else {
                m.strip_prefix(&format!("{root}/")).unwrap_or(m).to_string()
            };
            let d = match rest.split_once('/') {
                Some((seg, _)) => workspace::join(&root, seg),
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
        let idx: HashMap<&str, usize> = self
            .modules
            .iter()
            .enumerate()
            .map(|(i, m)| (*m, i))
            .collect();
        let adj: Vec<Vec<usize>> = self
            .modules
            .iter()
            .map(|m| self.out[m].iter().map(|t| idx[t]).collect())
            .collect();
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
        let active: Vec<&str> = self
            .modules
            .iter()
            .copied()
            .filter(|m| !self.use_in[m].is_empty() || !self.use_out[m].is_empty())
            .collect();
        let pct = |mut v: Vec<usize>| -> usize {
            if v.is_empty() {
                return r.hub_min;
            }
            v.sort();
            let i = ((r.hub_percentile * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1;
            v[i].max(r.hub_min)
        };
        (
            pct(active.iter().map(|m| self.use_in[m].len()).collect()),
            pct(active.iter().map(|m| self.use_out[m].len()).collect()),
        )
    }

    pub fn hubs(&self, th: (usize, usize)) -> Vec<&'g str> {
        self.modules
            .iter()
            .copied()
            .filter(|m| self.use_in[m].len() >= th.0 && self.use_out[m].len() >= th.1)
            .collect()
    }

    pub(super) fn top_level(&self, id: &'g str) -> Option<&'g str> {
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
            .children
            .get(m)
            .into_iter()
            .flatten()
            .filter(|n| n.kind != "test")
            .map(|n| n.id.as_str())
            .collect();
        if members.len() < 2 {
            return (members.len(), 0);
        }
        let ix: HashMap<&str, usize> = members.iter().enumerate().map(|(i, x)| (*x, i)).collect();
        let mut uf: Vec<usize> = (0..members.len()).collect();
        fn find(uf: &mut [usize], x: usize) -> usize {
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
        for e in self.into_file.get(m).into_iter().flatten() {
            if e.from == m {
                continue;
            }
            let Some(to) = self.top_level(&e.to).filter(|t| ix.contains_key(t)) else {
                continue;
            };
            let Some(from_node) = self.nodes.get(e.from.as_str()) else {
                continue;
            };
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

pub(super) fn tarjan(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
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
