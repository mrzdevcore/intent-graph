//! A design as a plan: its elements become new nodes, its marks changed/removed ones. Gaps are
//! what the design has and the plan lacks.

use super::{Design, Element, ElementKind, MarkKind};
use crate::config::Severity;
use crate::model::Finding;
use crate::plan::{Omitted, Plan, PlanEdge, PlanNode, PlanStatus};
use std::collections::{HashMap, HashSet};

/// The code-graph id an element stands for, if it has enough to say.
pub fn graph_id(e: &Element) -> Option<String> {
    let file = e.file.as_deref().filter(|f| !f.is_empty());
    match e.kind {
        ElementKind::Module => file.map(String::from),
        ElementKind::Function => file.map(|f| format!("{f}#{}", e.name)),
        ElementKind::Route => {
            let (m, p) = (e.method.as_deref()?, e.path.as_deref()?);
            file.map(|f| format!("{f}#{m} {p}"))
        }
        ElementKind::System => Some(format!("system:{}", e.name)),
        ElementKind::Note | ElementKind::Frame => None,
    }
}

#[derive(Debug)]
pub struct Planned {
    pub plan: Plan,
    /// element id -> graph id
    pub by_node: HashMap<String, String>,
    /// (graph from, graph to) -> link key
    pub by_edge: HashMap<(String, String), String>,
    pub findings: Vec<Finding>,
}

fn finding(rule: &str, sev: Severity, file: &str, id: &str, msg: String) -> Finding {
    Finding::new(rule, sev, "ig", file, 0, msg)
        .node(id)
        .fingerprinted()
}

/// An end of a link or bridge: an element id translates, anything else is already a graph id.
fn translate(by_node: &HashMap<String, String>, ids: &HashSet<&str>, end: &str) -> Option<String> {
    if ids.contains(end) {
        by_node.get(end).cloned()
    } else {
        Some(end.into())
    }
}

pub fn to_plan(d: &Design, path: &str) -> Planned {
    let mut p = Planned {
        plan: Plan {
            path: path.into(),
            title: d.title.clone(),
            done: false,
            steps: vec![],
            nodes: vec![],
            edges: vec![],
            systems: vec![],
            omitted: vec![],
        },
        by_node: HashMap::new(),
        by_edge: HashMap::new(),
        findings: vec![],
    };
    let mut seen = HashSet::new();
    for e in &d.elements {
        let Some(g) = graph_id(e) else {
            if !matches!(e.kind, ElementKind::Note | ElementKind::Frame) {
                p.findings.push(finding(
                    "design/incomplete",
                    Severity::Note,
                    path,
                    &e.id,
                    format!("{} ({}) has no file yet", e.id, e.name),
                ));
            }
            continue;
        };
        if !seen.insert(g.clone()) {
            p.findings.push(finding(
                "design/duplicate",
                Severity::Error,
                path,
                &e.id,
                format!(
                    "{} stands for {g}, which another element already does",
                    e.id
                ),
            ));
            continue;
        }
        p.by_node.insert(e.id.clone(), g.clone());
        if e.kind == ElementKind::System {
            p.plan.systems.push(e.name.clone());
        } else {
            p.plan.nodes.push(PlanNode {
                id: g,
                kind: Some(format!("{:?}", e.kind).to_lowercase()),
                status: PlanStatus::New,
                exported: false,
            });
        }
    }
    for m in &d.marks {
        let status = match m.mark {
            MarkKind::Changed => PlanStatus::Changed,
            MarkKind::Remove => PlanStatus::Removed,
        };
        p.plan.nodes.push(PlanNode {
            id: m.node.clone(),
            kind: None,
            status,
            exported: false,
        });
    }
    let ids: HashSet<&str> = d.elements.iter().map(|e| e.id.as_str()).collect();
    for l in &d.links {
        if let (Some(from), Some(to)) = (
            translate(&p.by_node, &ids, &l.from),
            translate(&p.by_node, &ids, &l.to),
        ) {
            p.plan.edges.push(PlanEdge {
                from: from.clone(),
                to: to.clone(),
                kind: l.kind.as_str().into(),
            });
            p.by_edge.insert((from, to), l.key());
        }
    }
    p
}

fn gap(file: &str, id: &str, what: &str) -> Finding {
    finding(
        "plan/design-gap",
        Severity::Error,
        file,
        id,
        format!("the design has {what} but the plan does not"),
    )
}

pub fn gaps(d: &Design, plan: &Plan) -> Vec<Finding> {
    let Planned { by_node, .. } = to_plan(d, &plan.path);
    let ids: HashSet<&str> = d.elements.iter().map(|e| e.id.as_str()).collect();
    let omitted: HashSet<&str> = plan.omitted.iter().map(|o| o.id.as_str()).collect();
    let planned = |g: &str| {
        plan.nodes.iter().any(|n| n.id == g)
            || g.strip_prefix("system:")
                .is_some_and(|s| plan.systems.iter().any(|x| x == s))
    };
    let edges: HashSet<(&str, &str)> = plan
        .edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    let mut out = vec![];
    for e in &d.elements {
        if let Some(g) = by_node.get(&e.id)
            && !planned(g)
            && !omitted.contains(e.id.as_str())
        {
            out.push(gap(&plan.path, &e.id, &format!("element {} ({g})", e.id)));
        }
    }
    for l in &d.links {
        let (Some(f), Some(t)) = (
            translate(&by_node, &ids, &l.from),
            translate(&by_node, &ids, &l.to),
        ) else {
            continue;
        };
        if !edges.contains(&(f.as_str(), t.as_str())) && !omitted.contains(l.key().as_str()) {
            out.push(gap(&plan.path, &l.key(), &format!("link {}", l.key())));
        }
    }
    for b in &d.bridges {
        let covered = translate(&by_node, &ids, &b.to).is_some_and(|t| planned(&t));
        if !covered && !omitted.contains(b.key().as_str()) {
            out.push(gap(&plan.path, &b.key(), &format!("bridge {}", b.key())));
        }
    }
    let known: HashSet<String> = ids
        .iter()
        .map(|s| s.to_string())
        .chain(d.links.iter().map(|l| l.key()))
        .chain(d.bridges.iter().map(|b| b.key()))
        .collect();
    for Omitted { id, .. } in &plan.omitted {
        if !known.contains(id) {
            out.push(finding(
                "plan/unknown-omitted",
                Severity::Warning,
                &plan.path,
                id,
                format!("omitted {id} is not in the design"),
            ));
        }
    }
    out
}

/// Plan nodes the design does not draw.
pub fn extras(d: &Design, plan: &Plan) -> Vec<String> {
    let drawn: HashSet<String> = d
        .elements
        .iter()
        .filter_map(graph_id)
        .chain(d.marks.iter().map(|m| m.node.clone()))
        .collect();
    plan.nodes
        .iter()
        .filter(|n| !drawn.contains(&n.id))
        .map(|n| n.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::tests::el;
    use super::super::*;
    use super::*;

    fn file(mut e: Element, f: &str) -> Element {
        e.file = Some(f.into());
        e
    }

    fn design() -> Design {
        let mut route = file(
            el("route-export", ElementKind::Route, "export"),
            "server/api/patients/export.get.ts",
        );
        (route.method, route.path) = (Some("GET".into()), Some("/api/patients/export".into()));
        let mut d = empty("t", Scope::Layer { name: "api".into() });
        d.elements = vec![
            route,
            file(
                el("fn-a", ElementKind::Function, "exportPatients"),
                "server/x.ts",
            ),
            el("sys-pg", ElementKind::System, "PostgreSQL"),
        ];
        d.marks = vec![Mark {
            node: "src/old.ts".into(),
            mark: MarkKind::Changed,
        }];
        d.links = vec![Link {
            from: "route-export".into(),
            to: "fn-a".into(),
            kind: LinkKind::Calls,
        }];
        d.bridges = vec![Bridge {
            from: "list/btn".into(),
            to: "route-export".into(),
        }];
        d
    }

    #[test]
    fn to_plan_maps_kinds_to_graph_ids() {
        let r = "server/api/patients/export.get.ts#GET /api/patients/export";
        let mut d = design();
        d.marks.push(Mark {
            node: "src/gone.ts".into(),
            mark: MarkKind::Remove,
        });
        let p = to_plan(&d, "d.json");
        assert_eq!(p.by_node["route-export"], r);
        assert_eq!(p.by_node["fn-a"], "server/x.ts#exportPatients");
        assert_eq!(p.plan.systems, ["PostgreSQL"]);
        let st = |id: &str| p.plan.nodes.iter().find(|n| n.id == id).map(|n| n.status);
        assert_eq!(st(r), Some(PlanStatus::New));
        assert_eq!(st("src/old.ts"), Some(PlanStatus::Changed));
        assert_eq!(st("src/gone.ts"), Some(PlanStatus::Removed));
        assert_eq!(p.plan.edges.len(), 1);
        assert_eq!(p.plan.edges[0].to, "server/x.ts#exportPatients");
        let key = &p.by_edge[&(r.to_string(), "server/x.ts#exportPatients".to_string())];
        assert_eq!(key, "route-export|calls|fn-a");
        assert!(p.findings.is_empty());
    }

    #[test]
    fn incomplete_and_duplicate_elements_become_findings() {
        let mut d = design();
        d.elements.push(el("mod-x", ElementKind::Module, "x"));
        d.elements.push(file(
            el("fn-dup", ElementKind::Function, "exportPatients"),
            "server/x.ts",
        ));
        let rules: Vec<_> = to_plan(&d, "d.json")
            .findings
            .iter()
            .map(|f| f.rule_id.clone())
            .collect();
        assert_eq!(rules, ["design/incomplete", "design/duplicate"]);
    }

    #[test]
    fn gaps_name_missing_elements_links_and_bridges_unless_omitted() {
        let mut d = design();
        d.elements.retain(|e| e.id != "sys-pg");
        d.elements.retain(|e| e.id != "route-export");
        d.links.clear();
        d.bridges = vec![Bridge {
            from: "list/btn".into(),
            to: "server/z.ts#z".into(),
        }];
        d.links.push(Link {
            from: "fn-a".into(),
            to: "server/z.ts#z".into(),
            kind: LinkKind::Calls,
        });
        let mut plan = to_plan(
            &Design {
                elements: vec![],
                links: vec![],
                bridges: vec![],
                marks: vec![],
                ..d.clone()
            },
            "p.md",
        )
        .plan;
        let n = |p: &Plan| {
            gaps(&d, p)
                .iter()
                .map(|f| f.rule_id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(n(&plan), ["plan/design-gap"; 3]);
        plan.omitted = vec![Omitted {
            id: "fn-a".into(),
            reason: "no".into(),
        }];
        assert_eq!(n(&plan), ["plan/design-gap"; 2]);
        plan.omitted.push(Omitted {
            id: "fn-zzz".into(),
            reason: "x".into(),
        });
        assert_eq!(n(&plan).last().unwrap(), "plan/unknown-omitted");
    }

    #[test]
    fn extras_lists_plan_nodes_the_design_does_not_have() {
        let d = design();
        let mut plan = to_plan(&d, "p.md").plan;
        plan.nodes.push(PlanNode {
            id: "server/x.ts#helper".into(),
            kind: None,
            status: PlanStatus::New,
            exported: false,
        });
        assert_eq!(extras(&d, &plan), ["server/x.ts#helper"]);
        assert!(gaps(&d, &plan).is_empty());
    }
}
