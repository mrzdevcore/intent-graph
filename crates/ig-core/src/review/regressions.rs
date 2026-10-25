//! Structural regressions `check` reports: new cycles, new hubs, cohesion drops.

use super::module_graph::ModuleGraph;
use super::{Ctx, chain, fingerprint};
use crate::config::Severity;
use crate::model::{Finding, Graph};
use std::collections::HashSet;

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
    let base_sccs: Vec<HashSet<&str>> = bg
        .sccs()
        .into_iter()
        .map(|c| c.into_iter().collect())
        .collect();
    for c in hg.sccs() {
        if !base_sccs.iter().any(|b| c.iter().all(|m| b.contains(m))) {
            let within: HashSet<&str> = c.iter().copied().collect();
            let lp = hg.shortest_loop(c[0], &within);
            out.push(mk(
                "struct/new-cycle",
                c.iter().map(|s| s.to_string()).collect(),
                format!("new import cycle: {}", chain(&lp)),
            ));
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
                format!(
                    "{h} became a hub: used by {} modules, uses {}",
                    hg.use_in[h].len(),
                    hg.use_out[h].len()
                ),
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
            out.push(mk(
                "struct/cohesion-drop",
                vec![m.to_string()],
                format!("{m} now does {jobs} unrelated jobs"),
            ));
        }
    }
    out
}
