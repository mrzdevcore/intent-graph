//! `ig review`: module graph, structural rules, ranking, and the structural regressions `check` uses.
//! Families: cycles, hubs, unstable dependencies, cohesion and oversized (here), layer cycles,
//! change coupling, unused exports, regressions (own modules).

mod coupling;
mod layer_cycles;
mod module_graph;
mod regressions;
mod unused;

pub use module_graph::ModuleGraph;
pub use regressions::regressions;

use crate::config::Config;
use crate::model::{Graph, Node, sha1_hex};
use crate::workspace::Package;
use coupling::change_coupling;
use globset::GlobSet;
use layer_cycles::layer_cycles;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use unused::unused_exports;

pub struct Ctx<'a> {
    pub config: &'a Config,
    pub tests: &'a GlobSet,
    pub packages: &'a [Package],
    pub entry_points: &'a GlobSet,
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

fn item(
    mg: &ModuleGraph,
    rule: &str,
    affected: usize,
    modules: Vec<String>,
    title: String,
    evidence: String,
    why: &str,
    fix: &str,
) -> ReviewItem {
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

/// `history`: the files each recent commit changed (for change coupling); empty without git.
pub fn review(graph: &Graph, ctx: &Ctx, history: &[Vec<String>]) -> Vec<ReviewItem> {
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
            format!(
                "used by {i} modules and uses {o} (thresholds {} / {})",
                th.0, th.1
            ),
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
        let (i, o) = (
            *dir_in.get(d).unwrap_or(&0) as f64,
            *dir_out.get(d).unwrap_or(&0) as f64,
        );
        if i + o == 0.0 { 0.0 } else { o / (i + o) }
    };
    for ((a, b), es) in &dir_edges {
        if *dir_in.get(a).unwrap_or(&0) < r.stable_fan_in || mg.source_roots.contains(*b) {
            continue;
        }
        if inst(b) >= inst(a) + r.instability_gap {
            let mods: Vec<String> = es
                .iter()
                .flat_map(|(f, t)| [f.to_string(), t.to_string()])
                .collect();
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
    items.extend(change_coupling(&mg, r, history));

    // Unused exports.
    items.extend(unused_exports(&mg, ctx));

    // Oversized.
    for m in &mg.modules {
        let n = mg.nodes[m];
        let big: Vec<&Node> = mg.by_file[n.file.as_str()]
            .iter()
            .copied()
            .filter(|x| !x.is_module() && x.metrics.complexity as usize >= r.oversized_complexity)
            .collect();
        if n.metrics.loc as usize >= r.oversized_loc || !big.is_empty() {
            let mut ev = vec![];
            if n.metrics.loc as usize >= r.oversized_loc {
                ev.push(format!("{} lines", n.metrics.loc));
            }
            for b in big.iter().take(3) {
                ev.push(format!(
                    "{} has complexity {}",
                    b.name, b.metrics.complexity
                ));
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

    items.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(b.fan_in.cmp(&a.fan_in))
            .then(a.fingerprint.cmp(&b.fingerprint))
    });
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

#[cfg(test)]
mod tests;
